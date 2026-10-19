pub mod locator;

use glam::{Quat, Vec3};
use gluon_ipc::{Handler, RefExt};
use locator::Locator;
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	ClientState, Context, Curve, CustomElement, Element, Entity, Migrate, Reify, Tasker,
	Transformable, client,
	components::Container,
	curve,
	elements::{Button, Model, Spatial, Text, Tracked, XAlign, YAlign},
	project_local_resources,
};
use stardust_xr_fusion::{
	client::{Client, DefaultHandler},
	fields::Shape,
	spatial::{SpatialRef, Transform},
	suis::Chirality,
	tracked::{self, TrackedExt, TrackedGuard, TrackedStateReceiver, TrackedStateReceiverHandler},
};
use std::{
	f32::consts::{FRAC_PI_2, PI},
	sync::Arc,
};

const ANIM_DURATION: f32 = 0.25;

/// a black hole is always pulling, the reach stays wide open so anything containable
/// keeps wanting to fall into it
const REACH: f32 = 1000.0;

/// how small the hole gets while everything is stuffed inside it, never zero or the
/// server inverts a singular transform and everything contained lands on NaN
const MIN_SCALE: f32 = 0.01;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt().with_file(false).init();

	client::run::<BlackHole>(&[&project_local_resources!("data")])
		.await
		.expect("Unable to connect to server");
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mount {
	Grip,
	Aim,
	Palm,
	Desk,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BlackHole {
	// nothing survives a restart, whatever was swallowed is gone by then anyway
	#[serde(skip)]
	closed: bool,
	#[serde(skip)]
	pressed_at: Option<f32>,
	/// where the button was when pressed, pinned so everything falls toward it instead
	/// of chasing the hand mid animation
	#[serde(skip)]
	hole: Vec3,
	#[serde(skip)]
	press: Option<(Mount, u32)>,
	#[serde(skip)]
	presses: u32,

	#[serde(skip)]
	grip: bool,
	#[serde(skip)]
	aim: bool,
	#[serde(skip)]
	palm: bool,
	/// only known once the runtime has a grip surface, the button rides the aim pose until then
	#[serde(skip)]
	grip_reach: Option<f32>,
}
impl Migrate for BlackHole {
	type Old = Self;
}
impl ClientState for BlackHole {
	const APP_ID: &'static str = "org.stardustxr.BlackHole";
}
impl BlackHole {
	fn busy(&self, time: f32) -> bool {
		self.pressed_at
			.is_some_and(|t| time - t < ANIM_DURATION * 2.0)
	}
	fn press(&mut self, mount: Mount, time: f32) {
		if self.busy(time) {
			return;
		}
		self.presses += 1;
		self.press = Some((mount, self.presses));
	}
	fn toggle(&mut self, hole: Vec3, time: f32) {
		self.hole = hole;
		self.pressed_at = Some(time);
		self.closed = !self.closed;
		self.press = None;
	}

	fn minimize(&self, time: f32, mount: Mount, transform: Transform) -> impl Element<Self> {
		Button::new(move |state: &mut Self| state.press(mount, time))
			.transform(transform)
			.size([0.02; 2])
			.build()
			.child(
				Text::new(if self.closed { "+" } else { "-" })
					.character_height(0.02)
					.align_x(XAlign::Center)
					.align_y(YAlign::Center)
					.build(),
			)
			.child(
				Locator::new(
					self.press.filter(|(m, _)| *m == mount).map(|(_, id)| id),
					move |state: &mut Self, hole| state.toggle(hole, time),
				)
				.build(),
			)
	}
}
impl Reify for BlackHole {
	fn reify(&self, context: &Context, tasks: impl Tasker<Self>, _props: ()) -> impl Element<Self> {
		let time = context.time();
		let pulse = self.pressed_at.is_some_and(|t| time - t < ANIM_DURATION);
		let client = context.stardust_client.clone();
		let face_up = Quat::from_rotation_x(PI + FRAC_PI_2);

		Spatial::default()
			.build()
			.child(
				Entity::new(Shape::Sphere { radius: REACH })
					.component(Container)
					.pos(if self.closed { self.hole } else { Vec3::ZERO })
					.scl([if self.closed { MIN_SCALE } else { 1.0 }; 3])
					.build()
					// the pull-in waits for the pulse to tip over, so the suck-in runs with the
					// visuals rather than ahead of them
					.animation(|a| {
						a.transform(if self.closed {
							expo_in(ANIM_DURATION).delay(ANIM_DURATION)
						} else {
							expo_out(ANIM_DURATION)
						})
					})
					.vanish(|a| a.transform(Transform::IDENTITY, expo_out(ANIM_DURATION))),
			)
			.child(
				Spatial::default().pos(self.hole).build().child(
					Spatial::default()
						.scl([if pulse { 1.0 } else { 0.0 }; 3])
						.build()
						.animation(|a| {
							a._0(if pulse {
								expo_out(ANIM_DURATION)
							} else {
								expo_in(ANIM_DURATION)
							})
						})
						.child(Model::namespaced("org.stardustxr.BlackHole", "black_hole").build()),
				),
			)
			.child(
				Tracked::controller_grip(Chirality::Left, move |state: &mut Self, tracked| {
					state.grip = tracked;
					if tracked {
						tasks.spawn(grip_reach(client.clone()), |_, state, reach| {
							state.grip_reach = reach.or(state.grip_reach)
						});
					}
				})
				.build()
				.maybe_child(self.grip_reach.filter(|_| self.grip).map(|x| {
					self.minimize(
						time,
						Mount::Grip,
						Transform::from_translation_rotation([x, 0.0, 0.0], face_up),
					)
				})),
			)
			.child(
				Tracked::controller_aim(Chirality::Left, |state: &mut Self, tracked| {
					state.aim = tracked
				})
				.build()
				.maybe_child((self.aim && self.grip_reach.is_none()).then(|| {
					self.minimize(
						time,
						Mount::Aim,
						Transform::from_translation_rotation([0.0, 0.01, 0.02], face_up),
					)
				})),
			)
			.child(
				Tracked::hand_palm(Chirality::Left, |state: &mut Self, tracked| {
					state.palm = tracked
				})
				.build()
				.maybe_child(self.palm.then(|| {
					self.minimize(
						time,
						Mount::Palm,
						Transform::from_translation_rotation(
							[0.0, 0.03, 0.0],
							Quat::from_rotation_x(-FRAC_PI_2),
						),
					)
				})),
			)
			.child(self.minimize(
				time,
				Mount::Desk,
				Transform::from_translation([0.0, 0.0, -0.3]),
			))
	}
}

fn expo_in(duration: f32) -> Curve {
	curve(move |t: f32| (t < duration).then(|| 2f32.powf(10.0 * (t / duration - 1.0))))
}
fn expo_out(duration: f32) -> Curve {
	curve(move |t: f32| (t < duration).then(|| 1.0 - 2f32.powf(-10.0 * t / duration)))
}

/// how far along the grip's x the button sits to clear the grip surface
async fn grip_reach(client: Arc<Client<DefaultHandler>>) -> Option<f32> {
	let (grip, _grip_guard) = tracked_space(
		tracked::Tracked::controller_grip(Chirality::Left)
			.await
			.ok()?,
	)
	.await?;
	let (surface, _surface_guard) = tracked_space(
		tracked::Tracked::controller_grip_surface(Chirality::Left)
			.await
			.ok()?,
	)
	.await?;
	let offset = client
		.spatial_interface()
		.get_relative_transform(grip, surface)
		.await
		.ok()?
		.ok()?;
	Some(0.04 - offset.translation.x)
}
async fn tracked_space(tracked: tracked::Tracked) -> Option<(SpatialRef, TrackedGuard)> {
	let receiver = TrackedStateReceiver::new_service(Untracked).ok()?;
	let (space, guard, _) = tracked.get(receiver).await.ok()?;
	Some((space, guard))
}

#[derive(Handler, Debug)]
struct Untracked;
impl TrackedStateReceiverHandler for Untracked {
	async fn tracked(&self, _ctx: gluon_ipc::Context, _tracked: bool) {}
}
