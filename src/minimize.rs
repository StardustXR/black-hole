use crate::black_hole::BlackHole;
use stardust_xr_fusion::{
	Result,
	client::{Client, ClientHandler},
	drawable::{Text, TextExt, TextStyle, XAlign, YAlign},
	spatial::{SpatialRef, Transform},
	types::rgba_linear,
};
use stardust_xr_molecules::{
	UIElement,
	button::{Button, ButtonSettings},
};

pub struct MinimizeButton {
	button: Button,
	button_ref: SpatialRef,
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
			black_hole.toggle(&self.button_ref);
		}
	}

	pub fn button_spatial_ref(&self) -> &SpatialRef {
		&self.button_ref
	}

	pub async fn new<H: ClientHandler>(
		client: &Client<H>,
		anchor: &SpatialRef,
		transform: Transform,
	) -> Result<Self> {
		let button = Button::new(
			client,
			anchor,
			transform,
			[0.02; 2].into(),
			ButtonSettings::default(),
		)
		.await?;
		let button_ref = button.touch_plane().root().spatial_ref().await?;
		let text = Text::new(
			client,
			button.touch_plane().root(),
			"-".to_string(),
			TextStyle {
				character_height: 0.02,
				color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
				text_align_x: XAlign::Center,
				text_align_y: YAlign::Top,
				font: None,
				bounds: None,
			},
		)
		.await?;

		Ok(MinimizeButton {
			button,
			button_ref,
			text,
			black_hole_was_open: true,
		})
	}
}
