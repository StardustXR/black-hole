use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	drawable::Model,
	fields::{Field, Shape},
	node::{NodeResult, NodeType},
	root::FrameInfo,
	spatial::{
		Spatial, SpatialAspect, SpatialRef, SpatialRefAspect, Transform, Zone, ZoneAspect,
		ZoneEvent,
	},
	values::ResourceID,
};
use tween::{ExpoIn, ExpoOut, Tweener};

pub enum AnimationState {
	Idle,
	Expand(Tweener<f32, f32, ExpoOut>),
	Contract(Tweener<f32, f32, ExpoIn>),
}

pub struct BlackHole {
	pub spatial: Spatial,
	field: Field,
	zone: Zone,
	_visuals: Model,
	open: bool,
	animation_state: AnimationState,
	entered: FxHashMap<u64, SpatialRef>,
	captured: FxHashMap<u64, Spatial>,
}
impl BlackHole {
	pub fn new(spatial_parent: &impl SpatialRefAspect) -> NodeResult<BlackHole> {
		let spatial = Spatial::create(spatial_parent, Transform::identity(), false)?;
		let radius = 10.0;
		let field = Field::create(&spatial, Transform::identity(), Shape::Sphere(radius))?;
		let zone = Zone::create(&spatial, Transform::from_scale([0.0; 3]), &field)?;

		let _visuals = Model::create(
			&field,
			Transform::from_scale([radius; 3]),
			&ResourceID::new_namespaced("black_hole", "black_hole"),
		)?;

		field.set_local_transform(Transform::from_scale([0.0001; 3]))?;

		Ok(BlackHole {
			spatial,
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
	pub fn frame(&mut self, info: &FrameInfo) {
		let _ = self.zone.update();
		while let Some(event) = self.zone.recv_zone_event() {
			match event {
				ZoneEvent::Enter { spatial } => {
					self.entered.insert(spatial.node().id(), spatial);
				}
				ZoneEvent::Capture { spatial } => {
					let _ = spatial.set_spatial_parent_in_place(&self.zone);
					self.captured.insert(spatial.node().id(), spatial);
				}
				ZoneEvent::Release { id } => {
					self.captured.remove(&id);
				}
				ZoneEvent::Leave { id } => {
					self.entered.remove(&id);
				}
			}
		}
		match &mut self.animation_state {
			AnimationState::Expand(e) => {
				self._visuals.set_enabled(true);
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
					self._visuals.set_enabled(false);
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
	pub fn open_now(&mut self) {
		let _ = self
			.zone
			.set_local_transform(Transform::from_scale([1.0; 3]));
		for captured in self.captured.values() {
			let _ = self.zone.release(captured);
		}
	}
}
