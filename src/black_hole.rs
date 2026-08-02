use glam::Vec3;
use gluon::{Handler, Interface};
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	Result,
	client::{Client, ClientHandler, FrameInfo},
	drawable::{Model, ModelExt},
	fields::{FieldRef, FieldSample},
	query::{InterfaceDependency, QueriedInterface, QueryableObjectRef},
	spatial::{PartialTransform, Spatial, SpatialExt, SpatialRef, Transform},
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

pub enum AnimationState {
	Idle,
	Expand(Tweener<f32, f32, ExpoOut>),
	Contract(Tweener<f32, f32, ExpoIn>),
}

#[derive(Debug, Handler)]
struct PointsHandler {
	in_zone: Mutex<FxHashMap<QueryableObjectRef, (ReparentableProxy, ReparentableLockedProxy)>>,
}
impl PointsQueryHandlerHandler for PointsHandler {
	async fn entered(
		&self,
		_ctx: gluon::Context,
		obj: QueryableObjectRef,
		_field: FieldRef,
		_spatial: SpatialRef,
		interfaces: Vec<QueriedInterface>,
		_spatial_info: FieldSample,
	) {
		tracing::info!(?obj, ?interfaces, "black hole: object entered center point");
		let Some(reparentable) = interfaces
			.iter()
			.find(|i| i.interface_id == ReparentableProxy::ID)
			.map(|i| ReparentableProxy::from_object_or_ref(i.interface.clone()))
		else {
			return;
		};
		let Some(reparentable_locked) = interfaces
			.iter()
			.find(|i| i.interface_id == ReparentableLockedProxy::ID)
			.map(|i| ReparentableLockedProxy::from_object_or_ref(i.interface.clone()))
		else {
			return;
		};
		self.in_zone
			.lock()
			.unwrap()
			.insert(obj, (reparentable, reparentable_locked));
	}
	async fn interfaces_changed(
		&self,
		_ctx: gluon::Context,
		obj: QueryableObjectRef,
		interfaces: Vec<QueriedInterface>,
	) {
		tracing::info!(?obj, ?interfaces, "black hole: object interfaces changed");
	}
	async fn moved(
		&self,
		_ctx: gluon::Context,
		_obj: QueryableObjectRef,
		_spatial_info: FieldSample,
	) {
	}
	async fn left(&self, _ctx: gluon::Context, obj: QueryableObjectRef) {
		tracing::info!(?obj, "black hole: object left center point");
		self.in_zone.lock().unwrap().remove(&obj);
	}
}

#[derive(Debug, Handler)]
struct BlackHoleKeepalive;
impl ReparentKeepaliveHandler for BlackHoleKeepalive {
	async fn reparent_stolen(&self, _ctx: gluon::Context) {}
}

pub struct BlackHole {
	root: SpatialRef,
	target_spatial: Spatial,
	target_ref: SpatialRef,
	reparent_spatial: Spatial,
	reparent_ref: SpatialRef,
	reparent_keepalive: gluon::Object<BlackHoleKeepalive>,
	// kept permanently at scale 1, tracking target_spatial's position, so the query's
	// reach never collapses along with target_spatial's animated 0..1 scale
	query_spatial: Spatial,
	_points_handle: PointsQueryHandle,
	points: gluon::Object<PointsHandler>,
	_visuals: Model,
	open: bool,
	animation_state: AnimationState,
	captured: (
		mpsc::UnboundedSender<ReparentHandle>,
		mpsc::UnboundedReceiver<ReparentHandle>,
	),
}
impl BlackHole {
	pub async fn new<H: ClientHandler>(
		client: &Client<H>,
		spatial_parent: &SpatialRef,
	) -> Result<BlackHole> {
		let (target_spatial, target_ref) =
			Spatial::new(client, spatial_parent, Transform::from_scale([0.0; 3])).await?;
		let (reparent_spatial, reparent_ref) =
			Spatial::new(client, &target_ref, Transform::IDENTITY).await?;
		let (query_spatial, query_ref) =
			Spatial::new(client, spatial_parent, Transform::IDENTITY).await?;

		// a single point at the black hole's center, with a huge margin so it still
		// catches everything reparentable regardless of distance, like a black hole should
		let points = client.pion_device().register_object(PointsHandler {
			in_zone: Mutex::default(),
		});
		let _points_handle = client
			.spatial_query_interface()
			.points_query(PointsQuery {
				handler: PointsQueryHandler::from_handler(&points),
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
			&target_spatial,
			Resource::Namespaced {
				namespace: "org.stardustxr.BlackHole".into(),
				path: "black_hole".into(),
			},
		)
		.await?;

		let reparent_keepalive = client.pion_device().register_object(BlackHoleKeepalive);
		Ok(BlackHole {
			root: client.root().clone(),
			target_spatial,
			target_ref,
			reparent_spatial,
			reparent_ref,
			query_spatial,
			_points_handle,
			points,
			_visuals,
			open: true,
			animation_state: AnimationState::Idle,
			captured: mpsc::unbounded_channel(),
			reparent_keepalive,
		})
	}
	pub fn open(&self) -> bool {
		self.open
	}
	pub fn in_transition(&self) -> bool {
		!matches!(&self.animation_state, AnimationState::Idle)
	}
	pub fn frame<H: ClientHandler>(&mut self, _client: &Client<H>, info: &FrameInfo) {
		match &mut self.animation_state {
			AnimationState::Expand(e) => {
				let scale = e.move_by(info.delta);
				let _ = self
					.target_spatial
					.set_local_transform(PartialTransform::from_scale([scale.max(0.0); 3]));

				if e.is_finished() {
					self.animation_state =
						AnimationState::Contract(Tweener::expo_in_at(1.0, 0.0, 0.25, 0.0));

					if self.open {
						// self.reparent_spatial.set_relative_transform(
						// 	self.root.clone(),
						// 	PartialTransform::from_scale([1.0; 3]),
						// );
						//                   sleep(Duration::from_millis(100));
						// opening: drop the locks, the reparentable objects take care of
						// putting themselves back where they belong on their own
						self.captured = mpsc::unbounded_channel();
						tracing::info!("dropping all captures");
					} else {
						// closing: capture everything currently reparentable
						let in_zone: Vec<_> = self
							.points
							.in_zone
							.lock()
							.unwrap()
							.iter()
							.map(|(k, v)| (k.clone(), v.clone()))
							.collect();
						for (_, (_reparentable, reparentable_locked)) in in_zone {
							let keepalive =
								ReparentKeepalive::from_handler(&self.reparent_keepalive);
							let sender = self.captured.0.clone();
							let reparent_ref = self.reparent_ref.clone();
							tokio::spawn(async move {
								let Some(handle) = reparentable_locked
									.reparent_locking(reparent_ref, keepalive)
									.await
									.ok()
									.flatten()
								else {
									return;
								};
								_ = sender.send(handle);
							});
						}
					}
				}
			}
			AnimationState::Contract(c) => {
				let scale = c.move_by(info.delta);
				let _ = self
					.target_spatial
					.set_local_transform(PartialTransform::from_scale([scale.max(0.0); 3]));

				if c.is_finished() {
					self.animation_state = AnimationState::Idle;
				}
			}
			_ => (),
		};
	}
	pub fn toggle(&mut self, target: &SpatialRef) {
		_ = self.query_spatial.set_relative_transform(
			target.clone(),
			PartialTransform::from_translation(Vec3::ZERO),
		);
		_ = self
			.target_spatial
			.set_local_transform(PartialTransform::from_scale(Vec3::ONE));
		_ = self.reparent_spatial.set_parent_in_place(self.root.clone());
		_ = self.target_spatial.set_relative_transform(
			target.clone(),
			PartialTransform::from_translation(Vec3::ZERO),
		);
		_ = self
			.reparent_spatial
			.set_parent_in_place(self.target_ref.clone());
		_ = self
			.target_spatial
			.set_local_transform(PartialTransform::from_scale(if self.open {
				Vec3::ONE
			} else {
				Vec3::ZERO
			}));
		self.open = !self.open;
		self.animation_state = AnimationState::Expand(Tweener::expo_out_at(0.0, 1.0, 0.25, 0.0));
	}
}
