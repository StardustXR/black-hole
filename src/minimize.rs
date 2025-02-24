use std::sync::mpsc;

use crate::black_hole::BlackHole;
use color_eyre::eyre::Result;
use stardust_xr_fusion::{
	drawable::{Text, TextAspect, TextStyle, XAlign, YAlign}, node::OwnedAspect, spatial::{SpatialRef, SpatialRefAspect, Transform}
};
use stardust_xr_molecules::{
	button::{Button, ButtonSettings},
	UIElement,
};

pub enum MinimizeButtonEvent {
	SetEnabled(bool),
}

pub struct MinimizeButton {
	button: Button,
	text: Text,
	black_hole_was_open: bool,
	events: mpsc::Receiver<MinimizeButtonEvent>,
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
		while let Ok(event) = self.events.try_recv() {
			match event {
				MinimizeButtonEvent::SetEnabled(is_tracked) => {
					_ = self.button.touch_plane().set_enabled(is_tracked);
					_ = self.text.set_enabled(is_tracked);
				}
			}
		}
		if self.button.released() && !black_hole.in_transition() {
			// let _ = black_hole.spatial.set_relative_transform(
			// 	self.button.touch_plane().root(),
			// 	Transform::from_translation([0.0, 0.0, -0.01]),
			// );
			black_hole.toggle();
		}
	}

	pub fn get_button_spatial_ref(&self) -> SpatialRef {
		self.button.touch_plane().root().clone().as_spatial_ref()
	}

	pub fn new(
		anchor: &impl SpatialRefAspect,
		transform: Transform,
	) -> Result<(Self, mpsc::Sender<MinimizeButtonEvent>)> {
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

		let (tx, rx) = mpsc::channel();
		Ok((
			MinimizeButton {
				button,
				text,
				black_hole_was_open: true,
				events: rx,
			},
			tx,
		))
	}
}
