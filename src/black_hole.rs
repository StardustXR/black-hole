use glam::Mat4;
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	drawable::Lines,
	fields::{Field, Shape},
	node::{NodeResult, NodeType},
	root::FrameInfo,
	spatial::{
		Spatial, SpatialAspect, SpatialRef, SpatialRefAspect, Transform, Zone, ZoneAspect,
		ZoneHandler,
	},
	values::color::rgba_linear,
	HandlerWrapper,
};
use stardust_xr_molecules::lines::{circle, LineExt};
use std::f32::consts::FRAC_PI_2;
use tween::{ExpoIn, ExpoOut, Tweener};

pub enum AnimationState {
	Idle,
	Expand(Tweener<f32, f32, ExpoOut>),
	Contract(Tweener<f32, f32, ExpoIn>),
}

pub struct BlackHole {
	field: Field,
	zone: Zone,
	_visuals: Lines,
	open: bool,
	animation_state: AnimationState,
	entered: FxHashMap<u64, SpatialRef>,
	captured: FxHashMap<u64, Spatial>,
}
impl BlackHole {
	pub fn create(
		spatial_parent: &impl SpatialRefAspect,
	) -> NodeResult<HandlerWrapper<Zone, BlackHole>> {
		let radius = 10.0;
		let field = Field::create(spatial_parent, Transform::identity(), Shape::Sphere(radius))?;
		let original_zone = Zone::create(spatial_parent, Transform::from_scale([0.0; 3]), &field)?;
		let zone = original_zone.alias();

		let circle = circle(32, 0.0, radius)
			.color(rgba_linear!(0.0, 1.0, 0.75, 1.0))
			.thickness(0.005);
		let _visuals = Lines::create(
			&field,
			Transform::identity(),
			&[
				circle.clone().transform(Mat4::from_rotation_x(FRAC_PI_2)),
				circle.clone().transform(Mat4::from_rotation_z(FRAC_PI_2)),
				circle,
			],
		)?;

		field.set_local_transform(Transform::from_scale([0.0001; 3]))?;

		original_zone.wrap(BlackHole {
			field,
			zone,
			_visuals,
			open: true,
			animation_state: AnimationState::Idle,
			entered: FxHashMap::default(),
			captured: FxHashMap::default(),
		})
	}
	pub fn open(&self) -> bool {
		self.open
	}
	pub fn in_transition(&self) -> bool {
		!matches!(&self.animation_state, AnimationState::Idle)
	}
	pub fn update(&mut self, info: &FrameInfo) {
		let _ = self.zone.update();
		match &mut self.animation_state {
			AnimationState::Expand(e) => {
				let scale = e.move_by(info.delta);

				if self.open {
					let _ = self
						.zone
						.set_local_transform(Transform::from_scale([scale; 3]));
				}
				let _ = self
					.field
					.set_local_transform(Transform::from_scale([scale.max(0.0001); 3]));
				if e.is_finished() {
					self.animation_state =
						AnimationState::Contract(Tweener::expo_in_at(1.0, 0.0, 0.25, 0.0));
					if self.open {
						for captured in self.captured.values() {
							let _ = self.zone.release(captured);
						}
					} else {
						for entered in self.entered.values() {
							let _ = self.zone.capture(entered);
						}
					}
				}
			}
			AnimationState::Contract(c) => {
				let scale = c.move_by(info.delta);
				if !self.open {
					let _ = self
						.zone
						.set_local_transform(Transform::from_scale([scale; 3]));
				}
				let _ = self
					.field
					.set_local_transform(Transform::from_scale([scale.max(0.0001); 3]));
				if c.is_finished() {
					self.animation_state = AnimationState::Idle;
				}
			}
			_ => (),
		};
	}
	pub fn toggle(&mut self) {
		self.open = !self.open;
		self.animation_state = AnimationState::Expand(Tweener::expo_out_at(0.0, 1.0, 0.25, 0.0));
	}
}
impl ZoneHandler for BlackHole {
	fn enter(&mut self, spatial: SpatialRef) {
		self.entered
			.insert(spatial.node().get_id().unwrap(), spatial);
	}

	fn capture(&mut self, spatial: Spatial) {
		let _ = spatial.set_spatial_parent_in_place(&self.zone);
		self.captured
			.insert(spatial.node().get_id().unwrap(), spatial);
	}
	fn release(&mut self, id: u64) {
		self.captured.remove(&id);
	}

	fn leave(&mut self, id: u64) {
		self.entered.remove(&id);
	}
}
