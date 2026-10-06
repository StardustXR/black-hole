pub mod black_hole;
pub mod minimize;

use std::{
	f32::consts::{FRAC_PI_2, PI},
	sync::OnceLock,
};

use black_hole::BlackHole;
use glam::Quat;
use gluon_ipc::{Handler, Liveness, Node};
use minimize::MinimizeButton;
use stardust_xr_fusion::{
	client::{Client, ClientHandler},
	project_local_resources,
	spatial::{PartialTransform, Spatial, SpatialExt, SpatialRef, Transform},
	suis::Chirality,
	tracked::{Tracked, TrackedExt, TrackedGuard, TrackedStateReceiverHandler},
};
use tokio::sync::broadcast::error::RecvError;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt().with_file(false).init();

	let (client, root) = Client::connect(&[&project_local_resources!("data")])
		.await
		.expect("Unable to connect to server");

	let mut black_hole = BlackHole::new(&client, &root)
		.await
		.expect("Unable to create black hole");

	let mut buttons = vec![];
	let xr_found = false;
	if let Some((anchor, transform, handle)) =
		controller_transform_ideal(&client, Chirality::Left).await
	{
		let button = MinimizeButton::new(&client, &anchor, transform)
			.await
			.expect("Unable to create minimize button");
		buttons.push((button, Some(handle)));
		// xr_found = true;
	} else if let Some((anchor, transform, handle)) =
		controller_transform_unideal(&client, Chirality::Left).await
	{
		let button = MinimizeButton::new(&client, &anchor, transform)
			.await
			.expect("Unable to create minimize button");
		buttons.push((button, Some(handle)));
		// xr_found = true;
	}
	if let Some((anchor, transform, handle)) = hand_transform(&client, Chirality::Left).await {
		let button = MinimizeButton::new(&client, &anchor, transform)
			.await
			.expect("Unable to create minimize button");
		buttons.push((button, Some(handle)));
		// xr_found = true;
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

		black_hole.frame(&info);
		for (button, _) in buttons.iter_mut() {
			button.frame(&mut black_hole).await;
		}
	}
}

pub async fn controller_transform_unideal(
	client: &Client<impl ClientHandler>,
	chirality: Chirality,
) -> Option<(SpatialRef, Transform, Node<MinimizingTracked>)> {
	let tracked = Tracked::controller(chirality).await.ok()?;
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
pub async fn controller_transform_ideal(
	client: &Client<impl ClientHandler>,
	chirality: Chirality,
) -> Option<(SpatialRef, Transform, Node<MinimizingTracked>)> {
	let tracked_palm = Tracked::controller_palm(chirality).await.ok()?;
	let tracked_grip = Tracked::controller_grip(chirality).await.ok()?;
	let (grip_tracked, grip_anchor) = MinimizingTracked::new(client, tracked_grip).await?;
	let (_, palm_anchor) = MinimizingTracked::new(client, tracked_palm).await?;

	let offset = client
		.spatial_interface()
		.get_relative_transform(grip_anchor.clone(), palm_anchor)
		.await
		.ok()?
		.ok()?;

	Some((
		grip_anchor,
		Transform::from_translation_rotation(
			[-offset.translation.x + 0.04, 0.0, 0.0],
			Quat::from_rotation_x(PI + FRAC_PI_2),
		),
		grip_tracked,
	))
}
pub async fn hand_transform(
	client: &Client<impl ClientHandler>,
	chirality: Chirality,
) -> Option<(SpatialRef, Transform, Node<MinimizingTracked>)> {
	let tracked = Tracked::hand(chirality).await.ok()?;
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
	async fn tracked(&self, _ctx: gluon_ipc::Context, tracked: bool) {
		_ = self
			.spatial
			.set_local_transform(PartialTransform::from_scale([tracked as u8 as f32; 3]));
	}
}
impl MinimizingTracked {
	pub async fn new(
		client: &Client<impl ClientHandler>,
		tracked: Tracked,
	) -> Option<(Node<Self>, SpatialRef)> {
		let (spatial, spatial_ref) = Spatial::new(client, client.root(), Transform::IDENTITY)
			.await
			.ok()?;
		let (node, tracked_state_receiver) = MinimizingTracked {
			spatial,
			guard: OnceLock::new(),
		}
		.to_node()
		.ok()?;
		let (tracked_spatial_ref, guard, tracked) =
			tracked.get(tracked_state_receiver).await.ok()?;
		node.spatial.set_parent(tracked_spatial_ref).ok()?;
		node.guard.set(guard).unwrap();
		node.spatial
			.set_local_transform(PartialTransform::from_scale([tracked as u8 as f32; 3]))
			.ok()?;
		Some((node, spatial_ref))
	}
}
