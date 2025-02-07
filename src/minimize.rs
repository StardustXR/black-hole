use crate::black_hole::BlackHole;
use color_eyre::eyre::Result;
use stardust_xr_fusion::{
	drawable::{Text, TextAspect, TextStyle, XAlign, YAlign},
	spatial::{SpatialAspect, SpatialRef, SpatialRefAspect, Transform},
};
use stardust_xr_molecules::{
	button::{Button, ButtonSettings},
	UIElement,
};

pub struct MinimizeButton {
	button: Button,
	text: Text,
	black_hole_was_open: bool,
}
impl MinimizeButton {
	pub fn frame(&mut self, black_hole: &mut BlackHole) {
		if black_hole.open() != self.black_hole_was_open {
			let _ = self
				.text
				.set_text(if black_hole.open() { "-" } else { "+" });
			self.black_hole_was_open = black_hole.open();
		}
		self.button.handle_events();
		if self.button.released() && !black_hole.in_transition() {
			// let _ = black_hole.spatial.set_relative_transform(
			// 	self.button.touch_plane().root(),
			// 	Transform::from_translation([0.0, 0.0, -0.01]),
			// );
			black_hole.toggle();
		}
	}

	pub async fn new(anchor: &impl SpatialRefAspect, transform: Transform) -> Result<Self> {
		let button = Button::create(anchor, transform, [0.02; 2], ButtonSettings::default())?;
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



		Ok(MinimizeButton {
			button,
			text,
			black_hole_was_open: true,
		})
	}
}
