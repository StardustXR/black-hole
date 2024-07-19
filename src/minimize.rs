use crate::black_hole::BlackHole;
use color_eyre::eyre::Result;
use stardust_xr_fusion::{
	drawable::{Text, TextAspect, TextStyle, XAlign, YAlign},
	node::MethodResult,
	root::{ClientState, FrameInfo, RootHandler},
	spatial::{Spatial, SpatialAspect, SpatialRefAspect, Transform, Zone},
	HandlerWrapper,
};
use stardust_xr_molecules::{
	button::{Button, ButtonSettings},
	DebugSettings, VisualDebug,
};

pub struct MinimizeButton {
	button: Button,
	text: Text,

	black_hole_parent: Spatial,
	black_hole: HandlerWrapper<Zone, BlackHole>,
}
impl MinimizeButton {
	pub async fn new(parent: &impl SpatialRefAspect, transform: Transform) -> Result<Self> {
		let mut button = Button::create(parent, transform, [0.02; 2], ButtonSettings::default())?;
		button.set_debug(Some(DebugSettings::default()));
		let text = Text::create(
			button.touch_plane().root(),
			Transform::identity(),
			"-",
			TextStyle {
				character_height: 0.02,
				text_align_x: XAlign::Center,
				text_align_y: YAlign::Top,
				..Default::default()
			},
		)?;
		let black_hole_parent = Spatial::create(
			parent.node().client()?.get_root(),
			Transform::identity(),
			false,
		)?;
		let black_hole = BlackHole::create(&black_hole_parent)?;

		Ok(MinimizeButton {
			button,
			text,

			black_hole_parent,
			black_hole,
		})
	}
}
impl RootHandler for MinimizeButton {
	fn frame(&mut self, info: FrameInfo) {
		let _ = self.black_hole_parent.set_relative_transform(
			self.button.touch_plane().root(),
			Transform::from_translation([0.0, 0.0, -0.01]),
			// Transform::from_translation([0.0, 0.0, info.elapsed.sin() * 0.1]),
		);

		self.button.update();
		if self.button.released() && !self.black_hole.lock_wrapped().in_transition() {
			let mut black_hole = self.black_hole.lock_wrapped();
			black_hole.toggle();
			let _ = self
				.text
				.set_text(if black_hole.open() { "-" } else { "+" });
		}

		self.black_hole.lock_wrapped().update(&info);
	}

	fn save_state(&mut self) -> MethodResult<ClientState> {
		Ok(ClientState::default())
	}
}
