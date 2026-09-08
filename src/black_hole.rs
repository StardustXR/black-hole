use glam::Vec3;
use gluon::Node;
use stardust_xr_fusion::{
	Result,
	client::{Client, ClientHandler, FrameInfo},
	drawable::{Model, ModelExt},
	fields::{Field, FieldExt, Shape},
	query::{QueryableExt, QueryableObject},
	spatial::{PartialTransform, Spatial, SpatialExt, SpatialInterface, SpatialRef, Transform},
	types::Resource,
};
use stardust_xr_molecules::container::Container;
use tween::{ExpoIn, ExpoOut, Tweener};

const ANIM_DURATION: f32 = 0.25;

/// a black hole is always pulling, the reach stays wide open so anything containable
/// keeps wanting to fall into it
const REACH: f32 = 1000.0;

/// how small the hole gets while everything is stuffed inside it, never zero or the
/// server inverts a singular transform and everything contained lands on NaN
const MIN_SCALE: f32 = 0.01;

/// Cosmetic pulse for the black hole's visuals: grows out then shrinks back, independent
/// of what any contained object is doing.
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

/// both tween how far down the hole everything is, 0 out in the world and 1 swallowed
enum Contain {
	/// waiting on the pulse to reach its apex before pulling everything down
	Pending,
	Shrinking(Tweener<f32, f32, ExpoIn>),
	Growing(Tweener<f32, f32, ExpoOut>),
}

pub struct BlackHole {
	// what containables reparent onto, carrying the hole's field so the reach shrinks in
	// lockstep with everything inside it and the samples keep their sign
	container_spatial: Spatial,
	_field: Field,
	_queryable: QueryableObject,
	_container: Node<Container>,
	spatial_interface: SpatialInterface,
	parent: SpatialRef,

	// carries the cosmetic pulse, uncoupled from anything contained
	visual_spatial: Spatial,
	_visuals: Model,

	open: bool,
	visual_pulse: Option<VisualPulse>,
	contain: Option<Contain>,
	fall: f32,
	button_position: Vec3,
}
impl BlackHole {
	pub async fn new<H: ClientHandler>(
		client: &Client<H>,
		spatial_parent: &SpatialRef,
	) -> Result<BlackHole> {
		let (container_spatial, _) =
			Spatial::new(client, spatial_parent, Transform::IDENTITY).await?;
		let (field, _) =
			Field::new(client, &container_spatial, Shape::Sphere { radius: REACH }).await?;

		let queryable =
			QueryableObject::new(client, container_spatial.clone(), field.clone()).await?;
		let container = Container::new(&queryable).await?;

		let (visual_spatial, _) =
			Spatial::new(client, spatial_parent, Transform::from_scale([0.0; 3])).await?;
		let _visuals = Model::new(
			client,
			&visual_spatial,
			Resource::Namespaced {
				namespace: "org.stardustxr.BlackHole".into(),
				path: "black_hole".into(),
			},
		)
		.await?;

		Ok(BlackHole {
			container_spatial,
			_field: field,
			_queryable: queryable,
			_container: container,
			spatial_interface: client.spatial_interface().clone(),
			parent: spatial_parent.clone(),
			visual_spatial,
			_visuals,
			open: true,
			visual_pulse: None,
			contain: None,
			fall: 0.0,
			button_position: Vec3::ZERO,
		})
	}
	pub fn open(&self) -> bool {
		self.open
	}
	pub fn in_transition(&self) -> bool {
		self.visual_pulse.is_some() || self.contain.is_some()
	}
	pub fn frame(&mut self, info: &FrameInfo) {
		if let Some(pulse) = &mut self.visual_pulse
			&& pulse.advance(&self.visual_spatial, info.delta)
		{
			self.visual_pulse = None;
		}

		let mut done = false;
		match &mut self.contain {
			// the pull-in only starts once the pulse tips over into contracting, so the
			// suck-in runs with the visuals rather than ahead of them
			Some(Contain::Pending)
				if !self
					.visual_pulse
					.as_ref()
					.is_some_and(VisualPulse::expanding) =>
			{
				self.contain = Some(Contain::Shrinking(Tweener::expo_in_at(
					self.fall,
					1.0,
					ANIM_DURATION,
					0.0,
				)));
				return;
			}
			Some(Contain::Shrinking(tween)) => {
				self.fall = tween.move_by(info.delta).clamp(0.0, 1.0);
				if tween.is_finished() {
					self.fall = 1.0;
					done = true;
				}
			}
			Some(Contain::Growing(tween)) => {
				self.fall = tween.move_by(info.delta).clamp(0.0, 1.0);
				if tween.is_finished() {
					self.fall = 0.0;
					done = true;
				}
			}
			_ => return,
		}
		let _ =
			self.container_spatial
				.set_local_transform(PartialTransform::from_translation_scale(
					self.button_position * self.fall,
					[1.0 + (MIN_SCALE - 1.0) * self.fall; 3],
				));
		if done {
			self.contain = None;
		}
	}
	pub async fn toggle(&mut self, target: &SpatialRef) {
		let _ = self.visual_spatial.set_relative_transform(
			target.clone(),
			PartialTransform::from_translation(Vec3::ZERO),
		);
		self.visual_pulse = Some(VisualPulse::new());

		if self.open {
			// pinned once here rather than tracked, everything falls toward where the
			// button was when you pressed it instead of chasing the hand mid animation
			if let Ok(Ok(transform)) = self
				.spatial_interface
				.get_relative_transform(self.parent.clone(), target.clone())
				.await
			{
				self.button_position = transform.translation.into();
			}
			self.contain = Some(Contain::Pending);
		} else {
			self.contain = Some(Contain::Growing(Tweener::expo_out_at(
				self.fall,
				0.0,
				ANIM_DURATION,
				0.0,
			)));
		}
		self.open = !self.open;
	}
}
