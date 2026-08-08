pub mod black_hole;
pub mod minimize;

use std::{
	f32::consts::{FRAC_PI_2, PI},
	sync::OnceLock,
};

use black_hole::BlackHole;
use glam::Quat;
use gluon::{Handler, Liveness, Object};
use minimize::MinimizeButton;
use stardust_xr_fusion::{
	client::{Client, ClientHandler},
	project_local_resources,
	spatial::{PartialTransform, Spatial, SpatialExt, SpatialRef, Transform},
	suis::Chirality,
	tracked::{
		Tracked, TrackedExt, TrackedGuard, TrackedStateReceiver, TrackedStateReceiverHandler,
	},
};
use tokio::sync::broadcast::error::RecvError;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt().with_file(false).init();

	let (client, root) = Client::auto_connect(&[&project_local_resources!("data")])
		.await
		.expect("Unable to connect to server");

	let mut black_hole = BlackHole::new(&client, &root)
		.await
		.expect("Unable to create black hole");

	// TODO: anchor the minimize button to the hand/controller via
	// controller_transform/hand_transform below once the server exposes tracked
	// objects for them through stardust_xr_fusion::tracked (currently only
	// "stardust-hmd" and "stardust-stage" are implemented server-side).
	// Until then it's always anchored to root.
	let mut buttons = vec![];
	let mut xr_found = false;
	if let Some((anchor, transform, handle)) = controller_transform(&client, Chirality::Left).await
	{
		let button = MinimizeButton::new(&client, &anchor, transform)
			.await
			.expect("Unable to create minimize button");
		buttons.push((button, Some(handle)));
		xr_found = true;
	}
	if let Some((anchor, transform, handle)) = hand_transform(&client, Chirality::Left).await {
		let button = MinimizeButton::new(&client, &anchor, transform)
			.await
			.expect("Unable to create minimize button");
		buttons.push((button, Some(handle)));
		xr_found = true;
	}
	if !xr_found {
		let button = MinimizeButton::new(
			&client,
			&root,
			Transform::from_translation([0.0, 0.0, -0.3]),
		)
		.await
		.expect("Unable to create minimize button");
		buttons.push((button, None));
	}

	let mut recv = client.frame_receiver();
	let server = client.server();
	loop {
		let info = tokio::select! {
			f = recv.recv() => {
				match f {
					Ok(info) => info,
					Err(RecvError::Closed) => break,
					Err(RecvError::Lagged(_)) => continue,
				}
			}
			_ = server.death_notification() => break,
		};

		black_hole.frame(&client, &info);
		for (button, _) in buttons.iter_mut() {
			button.frame(&mut black_hole).await;
		}
	}
}

// TODO: port these to the new stardust_xr_fusion::tracked API once the server
// implements tracked objects for controllers/hands (see the TODO in main above).
// Kept around as reference for the dbus-based lookup this used to do.
pub async fn controller_transform(
	client: &Client<impl ClientHandler>,
	chirality: Chirality,
) -> Option<(SpatialRef, Transform, Object<MinimizingTracked>)> {
	let tracked = Tracked::controller(client, chirality).await.ok()?;
	let (tracked, anchor) = MinimizingTracked::new(client, tracked).await?;

	Some((
		anchor,
		Transform::from_translation_rotation(
			[0.0, 0.01, 0.02],
			Quat::from_rotation_x(PI + FRAC_PI_2),
		),
		tracked,
	))
}
pub async fn hand_transform(
	client: &Client<impl ClientHandler>,
	chirality: Chirality,
) -> Option<(SpatialRef, Transform, Object<MinimizingTracked>)> {
	let tracked = Tracked::hand(client, chirality).await.ok()?;
	let (tracked, anchor) = MinimizingTracked::new(client, tracked).await?;

	Some((
		anchor,
		Transform::from_translation_rotation([0.0, 0.03, 0.0], Quat::from_rotation_x(-FRAC_PI_2)),
		tracked,
	))
}

#[derive(Handler, Debug)]
pub struct MinimizingTracked {
	spatial: Spatial,
	guard: OnceLock<TrackedGuard>,
}
impl TrackedStateReceiverHandler for MinimizingTracked {
	async fn tracked(&self, _ctx: gluon::Context, tracked: bool) {
		_ = self
			.spatial
			.set_local_transform(PartialTransform::from_scale([tracked as u8 as f32; 3]));
	}
}
impl MinimizingTracked {
	pub async fn new(
		client: &Client<impl ClientHandler>,
		tracked: Tracked,
	) -> Option<(Object<Self>, SpatialRef)> {
		let (spatial, spatial_ref) = Spatial::new(client, client.root(), Transform::IDENTITY)
			.await
			.ok()?;
		let obj = client.pion_device().register_object(Self {
			spatial,
			guard: OnceLock::new(),
		});
		let (tracked_spatial_ref, guard, tracked) = tracked
			.get(TrackedStateReceiver::from_handler(&obj))
			.await
			.ok()?;
		obj.spatial.set_parent(tracked_spatial_ref).ok()?;
		obj.guard.set(guard).unwrap();
		obj.spatial
			.set_local_transform(PartialTransform::from_scale([tracked as u8 as f32; 3]))
			.ok()?;
		Some((obj, spatial_ref))
	}
}
