pub mod black_hole;
pub mod minimize;

use black_hole::BlackHole;
use gluon::Liveness;
use minimize::MinimizeButton;
use stardust_xr_fusion::{client::Client, project_local_resources, spatial::Transform};
use tokio::sync::broadcast::error::RecvError;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt().pretty().with_file(false).init();

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
	let button = MinimizeButton::new(
		&client,
		&root,
		Transform::from_translation([0.0, 0.0, -0.3]),
	)
	.await
	.expect("Unable to create minimize button");
	let mut buttons = [button];

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
		for button in buttons.iter_mut() {
			button.frame(&mut black_hole);
		}
	}
}

// TODO: port these to the new stardust_xr_fusion::tracked API once the server
// implements tracked objects for controllers/hands (see the TODO in main above).
// Kept around as reference for the dbus-based lookup this used to do.
//
// pub async fn controller_transform(
// 	client: &Arc<ClientHandle>,
// 	conn: &Connection,
// ) -> Option<(SpatialRef, Transform, TrackedProxy<'static>)> {
// 	let anchor = SpatialRefProxy::new(
// 		conn,
// 		WellKnownName::from_static_str("org.stardustxr.Controllers").ok()?,
// 		"/org/stardustxr/Controller/left",
// 	)
// 	.await
// 	.ok()?
// 	.import(client)
// 	.await?;
// 	let tracked = TrackedProxy::new(
// 		conn,
// 		WellKnownName::from_static_str("org.stardustxr.Controllers").ok()?,
// 		"/org/stardustxr/Controller/left",
// 	)
// 	.await
// 	.ok()?;
//
// 	Some((
// 		anchor,
// 		Transform::from_translation_rotation(
// 			[0.0, 0.01, 0.02],
// 			Quat::from_rotation_x(PI + FRAC_PI_2),
// 		),
// 		tracked,
// 	))
// }
// pub async fn hand_transform(
// 	client: &Arc<ClientHandle>,
// 	conn: &Connection,
// ) -> Option<(SpatialRef, Transform, TrackedProxy<'static>)> {
// 	let anchor = stardust_xr_fusion::objects::interfaces::SpatialRefProxy::new(
// 		conn,
// 		WellKnownName::from_static_str("org.stardustxr.Hands").ok()?,
// 		"/org/stardustxr/Hand/left/palm",
// 	)
// 	.await
// 	.ok()?
// 	.import(client)
// 	.await?;
// 	let tracked = TrackedProxy::new(
// 		conn,
// 		WellKnownName::from_static_str("org.stardustxr.Hands").ok()?,
// 		"/org/stardustxr/Hand/left",
// 	)
// 	.await
// 	.ok()?;
//
// 	Some((
// 		anchor,
// 		Transform::from_translation_rotation([0.0, 0.03, 0.0], Quat::from_rotation_x(-FRAC_PI_2)),
// 		tracked,
// 	))
// }
