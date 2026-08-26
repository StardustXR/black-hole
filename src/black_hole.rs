use glam::Vec3;
use gluon::{Handler, Interface, LocalRef, Node, RefExt};
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	Result,
	client::{Client, ClientHandler, FrameInfo},
	drawable::{Model, ModelExt},
	fields::{FieldRef, FieldSample},
	query::{InterfaceDependency, QueriedInterface, QueryableId},
	spatial::{PartialTransform, Spatial, SpatialExt, SpatialInterface, SpatialRef, Transform},
	spatial_query::{
		Point, PointsQuery, PointsQueryHandle, PointsQueryHandler, PointsQueryHandlerHandler,
	},
	types::Resource,
};
use stardust_xr_molecules::reparentable::{
	ReparentHandle, ReparentKeepalive, ReparentKeepaliveHandler, ReparentableLockedProxy,
	ReparentableProxy,
};
use std::sync::Mutex;
use tokio::sync::mpsc;
use tween::{ExpoIn, ExpoOut, Tweener};

const ANIM_DURATION: f32 = 0.25;

/// Cosmetic pulse for the black hole's visuals: grows out then shrinks back, independent
/// of what any captured object is doing.
pub enum VisualPulse {
	Expand(Tweener<f32, f32, ExpoOut>),
	Contract(Tweener<f32, f32, ExpoIn>),
}
impl VisualPulse {
	/// True until the pulse crosses its apex and starts contracting.
	fn expanding(&self) -> bool {
		matches!(self, VisualPulse::Expand(_))
	}
	fn new() -> Self {
		VisualPulse::Expand(Tweener::expo_out_at(0.0, 1.0, ANIM_DURATION, 0.0))
	}
	/// Returns true once the pulse is over.
	fn advance(&mut self, visual_spatial: &Spatial, delta: f32) -> bool {
		match self {
			VisualPulse::Expand(e) => {
				let scale = e.move_by(delta);
				let _ = visual_spatial
					.set_local_transform(PartialTransform::from_scale([scale.max(0.0); 3]));
				if e.is_finished() {
					*self =
						VisualPulse::Contract(Tweener::expo_in_at(1.0, 0.0, ANIM_DURATION, 0.0));
				}
				false
			}
			VisualPulse::Contract(c) => {
				let scale = c.move_by(delta);
				let _ = visual_spatial
					.set_local_transform(PartialTransform::from_scale([scale.max(0.0); 3]));
				c.is_finished()
			}
		}
	}
}

enum CaptureAnim {
	/// Captured and held at full scale, waiting for the visual pulse to reach its apex.
	Pending,
	Shrinking(Tweener<f32, f32, ExpoIn>),
	Held,
	Growing(Tweener<f32, f32, ExpoOut>),
	/// Parked at exactly scale 1 for one frame so the transform lands before the handle
	/// drop reparents the object back out.
	Releasing,
}

/// One captured object and the holder spatial it hangs from. The holder is created at
/// scale 1 and only ever animated by us, so the object's reparent always bakes against a
/// scale of exactly 1 no matter when the RPC lands, and always releases at exactly 1.
struct Capture {
	holder: Spatial,
	_handle: ReparentHandle,
	scale: f32,
	anim: CaptureAnim,
}
impl Capture {
	fn animating(&self) -> bool {
		!matches!(self.anim, CaptureAnim::Held)
	}
	fn shrink(&mut self) {
		self.anim =
			CaptureAnim::Shrinking(Tweener::expo_in_at(self.scale, 0.0, ANIM_DURATION, 0.0));
	}
	fn grow(&mut self) {
		self.anim = CaptureAnim::Growing(Tweener::expo_out_at(self.scale, 1.0, ANIM_DURATION, 0.0));
	}
	/// Returns false once this capture is done and should be dropped, releasing the object.
	fn advance(&mut self, delta: f32) -> bool {
		let mut next = None;
		match &mut self.anim {
			CaptureAnim::Shrinking(t) => {
				self.scale = t.move_by(delta).max(0.0);
				if t.is_finished() {
					next = Some(CaptureAnim::Held);
				}
			}
			CaptureAnim::Growing(t) => {
				self.scale = t.move_by(delta).max(0.0);
				if t.is_finished() {
					self.scale = 1.0;
					next = Some(CaptureAnim::Releasing);
				}
			}
			CaptureAnim::Pending | CaptureAnim::Held => return true,
			CaptureAnim::Releasing => return false,
		}
		if let Some(next) = next {
			self.anim = next;
		}
		let _ = self
			.holder
			.set_local_transform(PartialTransform::from_scale([self.scale; 3]));
		true
	}
}

/// A capture that finished reparenting, on its way back to the main loop.
struct LandedCapture {
	id: QueryableId,
	holder: Spatial,
	handle: ReparentHandle,
}

#[derive(Debug, Handler)]
struct PointsHandler {
	in_zone: Mutex<FxHashMap<QueryableId, (ReparentableProxy, ReparentableLockedProxy)>>,
}
impl PointsQueryHandlerHandler for PointsHandler {
	async fn entered(
		&self,
		_ctx: gluon::Context,
		id: QueryableId,
		_field: FieldRef,
		_spatial: SpatialRef,
		interfaces: Vec<QueriedInterface>,
		_spatial_info: FieldSample,
	) {
		tracing::info!(?id, ?interfaces, "black hole: object entered center point");
		let Some(reparentable) = interfaces
			.iter()
			.find(|i| i.interface_id == ReparentableProxy::ID)
			.map(|i| ReparentableProxy::from_ref(i.interface.clone()))
		else {
			return;
		};
		let Some(reparentable_locked) = interfaces
			.iter()
			.find(|i| i.interface_id == ReparentableLockedProxy::ID)
			.map(|i| ReparentableLockedProxy::from_ref(i.interface.clone()))
		else {
			return;
		};
		self.in_zone
			.lock()
			.unwrap()
			.insert(id, (reparentable, reparentable_locked));
	}
	async fn interfaces_changed(
		&self,
		_ctx: gluon::Context,
		id: QueryableId,
		interfaces: Vec<QueriedInterface>,
	) {
		tracing::info!(?id, ?interfaces, "black hole: object interfaces changed");
	}
	async fn moved(&self, _ctx: gluon::Context, _id: QueryableId, _spatial_info: FieldSample) {}
	async fn left(&self, _ctx: gluon::Context, id: QueryableId) {
		tracing::info!(?id, "black hole: object left center point");
		self.in_zone.lock().unwrap().remove(&id);
	}
}

#[derive(Debug, Handler)]
struct BlackHoleKeepalive;
impl ReparentKeepaliveHandler for BlackHoleKeepalive {
	async fn reparent_stolen(&self, _ctx: gluon::Context) {}
}

pub struct BlackHole {
	spatial_interface: SpatialInterface,
	// pinned at scale 1, tracking the hole's position: parent of every capture holder, and
	// the query's reference so its reach never collapses
	query_spatial: Spatial,
	query_ref: SpatialRef,
	// carries the cosmetic pulse, uncoupled from anything captured
	visual_spatial: Spatial,
	_reparent_keepalive: Node<BlackHoleKeepalive>,
	reparent_keepalive_ref: LocalRef<ReparentKeepalive, BlackHoleKeepalive>,
	_points_handle: PointsQueryHandle,
	points_query_handler: Node<PointsHandler>,
	_visuals: Model,
	open: bool,
	visual_pulse: Option<VisualPulse>,
	captures: FxHashMap<QueryableId, Capture>,
	landed: (
		mpsc::UnboundedSender<LandedCapture>,
		mpsc::UnboundedReceiver<LandedCapture>,
	),
}
impl BlackHole {
	pub async fn new<H: ClientHandler>(
		client: &Client<H>,
		spatial_parent: &SpatialRef,
	) -> Result<BlackHole> {
		let (query_spatial, query_ref) =
			Spatial::new(client, spatial_parent, Transform::IDENTITY).await?;
		let (visual_spatial, _visual_ref) =
			Spatial::new(client, spatial_parent, Transform::from_scale([0.0; 3])).await?;

		// a single point at the black hole's center, with a huge margin so it still
		// catches everything reparentable regardless of distance, like a black hole should
		let (points_query_handler, points_handler_ref) =
			PointsQueryHandler::new_node(PointsHandler {
				in_zone: Mutex::default(),
			})?;
		let _points_handle = client
			.spatial_query_interface()
			.points_query(PointsQuery {
				handler: points_handler_ref.into_proxy(),
				interfaces: vec![
					InterfaceDependency {
						id: ReparentableProxy::ID.into(),
						optional: false,
					},
					InterfaceDependency {
						id: ReparentableLockedProxy::ID.into(),
						optional: false,
					},
				],
				reference_spatial: query_ref.clone(),
				points: vec![Point {
					point: [0.0, 0.0, 0.0].into(),
					margin: f32::MAX,
				}],
			})
			.await?
			.unwrap();
		tracing::info!("black hole: points query registered, watching for reparentable objects");

		let _visuals = Model::new(
			client,
			&visual_spatial,
			Resource::Namespaced {
				namespace: "org.stardustxr.BlackHole".into(),
				path: "black_hole".into(),
			},
		)
		.await?;

		let (reparent_keepalive, reparent_keepalive_ref) =
			ReparentKeepalive::new_node(BlackHoleKeepalive)?;
		Ok(BlackHole {
			spatial_interface: client.spatial_interface().clone(),
			query_spatial,
			query_ref,
			visual_spatial,
			_points_handle,
			points_query_handler,
			_visuals,
			open: true,
			visual_pulse: None,
			captures: FxHashMap::default(),
			landed: mpsc::unbounded_channel(),
			_reparent_keepalive: reparent_keepalive,
			reparent_keepalive_ref,
		})
	}
	pub fn open(&self) -> bool {
		self.open
	}
	pub fn in_transition(&self) -> bool {
		self.visual_pulse.is_some() || self.captures.values().any(Capture::animating)
	}
	pub fn frame<H: ClientHandler>(&mut self, _client: &Client<H>, info: &FrameInfo) {
		while let Ok(landed) = self.landed.1.try_recv() {
			if self.open {
				// missed its window; the holder is still at scale 1 so dropping it here
				// puts the object back exactly as it was
				continue;
			}
			self.captures.insert(
				landed.id,
				Capture {
					holder: landed.holder,
					_handle: landed.handle,
					scale: 1.0,
					anim: CaptureAnim::Pending,
				},
			);
		}
		if let Some(pulse) = &mut self.visual_pulse
			&& pulse.advance(&self.visual_spatial, info.delta)
		{
			self.visual_pulse = None;
		}
		// captures only start shrinking once the pulse tips over into contracting, so the
		// suck-in runs with the visuals rather than ahead of them
		let past_apex = !self
			.visual_pulse
			.as_ref()
			.is_some_and(VisualPulse::expanding);
		if past_apex && !self.open {
			for capture in self.captures.values_mut() {
				if matches!(capture.anim, CaptureAnim::Pending) {
					capture.shrink();
				}
			}
		}
		self.captures
			.retain(|_, capture| capture.advance(info.delta));
	}
	pub fn toggle(&mut self, target: &SpatialRef) {
		let _ = self.visual_spatial.set_relative_transform(
			target.clone(),
			PartialTransform::from_translation(Vec3::ZERO),
		);
		self.visual_pulse = Some(VisualPulse::new());

		if self.open {
			// nothing is captured yet, so the hole is free to move to the button
			let _ = self.query_spatial.set_relative_transform(
				target.clone(),
				PartialTransform::from_translation(Vec3::ZERO),
			);
			self.capture_all();
		} else {
			for capture in self.captures.values_mut() {
				capture.grow();
			}
		}
		self.open = !self.open;
	}
	/// Spawns a capture task per object in the zone. Each builds its own holder spatial at
	/// scale 1 and reparents onto that, so however long the round trip takes, the object
	/// lands at its true size and starts its own shrink whenever it arrives.
	fn capture_all(&self) {
		let in_zone: Vec<_> = self
			.points_query_handler
			.in_zone
			.lock()
			.unwrap()
			.iter()
			.map(|(k, v)| (*k, v.1.clone()))
			.collect();
		for (id, reparentable_locked) in in_zone {
			if self.captures.contains_key(&id) {
				continue;
			}
			let keepalive = self.reparent_keepalive_ref.proxy().clone();
			let sender = self.landed.0.clone();
			let spatial_interface = self.spatial_interface.clone();
			let query_ref = self.query_ref.clone();
			tokio::spawn(async move {
				let Ok(Ok(holder)) = spatial_interface
					.create_spatial(query_ref, Transform::IDENTITY)
					.await
				else {
					return;
				};
				let handle = reparentable_locked
					.reparent_locking(holder.spatial_ref, keepalive)
					.await
					.ok()
					.flatten();
				let Some(handle) = handle else {
					return;
				};
				let _ = sender.send(LandedCapture {
					id,
					holder: holder.spatial,
					handle,
				});
			});
		}
	}
}
