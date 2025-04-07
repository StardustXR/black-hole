pub mod black_hole;
pub mod minimize;

use black_hole::BlackHole;
use color_eyre::eyre::Result;
use glam::Quat;
use manifest_dir_macros::directory_relative_path;
use minimize::{MinimizeButton, MinimizeButtonEvent};
use stardust_xr_fusion::{
	client::Client,
	core::schemas::zbus::{names::WellKnownName, Connection},
	objects::SpatialRefProxyExt,
	root::{RootAspect, RootEvent},
	spatial::{SpatialRef, Transform},
	ClientHandle,
};
use stardust_xr_molecules::tracked::TrackedProxy;
use std::{
	f32::consts::{FRAC_PI_2, PI},
	sync::{mpsc, Arc},
};
use tokio_stream::StreamExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let client = Client::connect()
		.await
		.expect("Unable to connect to server");
	let client_handle = client.handle();
	let async_loop = client.async_event_loop();
	client_handle
		.get_root()
		.set_base_prefixes(&[directory_relative_path!("res").to_owned()])?;

	let mut black_hole = BlackHole::new(client_handle.get_root())?;
	let mut buttons: [Option<MinimizeButton>; 2] = [None, None];
	let mut was_spawned = false;
	if let Some((anchor, offset, tracked)) = controller_transform(&client_handle).await {
		was_spawned = true;
		let (button, tx) = MinimizeButton::new(&anchor, offset)?;
		update_tracked_state(tracked, tx);
		buttons[0] = Some(button);
	};
	if let Some((anchor, offset, tracked)) = hand_transform(&client_handle).await {
		was_spawned = true;
		let (button, tx) = MinimizeButton::new(&anchor, offset)?;
		update_tracked_state(tracked, tx);
		buttons[1] = Some(button);
	}
	if !was_spawned {
		println!("hitting the fallback! :3 ?");
		buttons[0] = Some(
			MinimizeButton::new(
				client_handle.get_root(),
				Transform::from_translation([0.0, 0.0, -0.3]),
			)?
			.0,
		);
	};
	let mut client = async_loop.stop().await?;
	let loop_future = client.sync_event_loop(|client, _| {
		while let Some(event) = client.get_root().recv_root_event() {
			match event {
				RootEvent::Frame { info } => {
					black_hole.frame(&info);
					for button in buttons.iter_mut().filter_map(Option::as_mut) {
						button.frame(&mut black_hole);
					}
				}
				RootEvent::SaveState { response: _ } => {}
				RootEvent::Ping { response } => {
					response.send(Ok(()));
				}
			}
		}
	});
	tokio::select! {
		_ = loop_future => {},
		_ = tokio::signal::ctrl_c() => {}
	};
	black_hole.open_now();
	_ = client.try_flush().await;
	Ok(())
}

fn update_tracked_state(tracked: TrackedProxy<'static>, tx: mpsc::Sender<MinimizeButtonEvent>) {
	tokio::spawn(async move {
		if let Ok(is_tracked) = tracked.is_tracked().await {
			_ = tx.send(MinimizeButtonEvent::SetEnabled(is_tracked));
		}
		let mut stream = tracked.receive_is_tracked_changed().await;
		while let Some(value) = stream.next().await {
			if let Ok(is_tracked) = value.get().await {
				_ = tx.send(MinimizeButtonEvent::SetEnabled(is_tracked));
			}
		}
	});
}

pub async fn controller_transform(
	client: &Arc<ClientHandle>,
) -> Option<(SpatialRef, Transform, TrackedProxy<'static>)> {
	let anchor = stardust_xr_fusion::objects::interfaces::SpatialRefProxy::new(
		&Connection::session().await.ok()?,
		WellKnownName::from_static_str("org.stardustxr.Controllers").ok()?,
		"/org/stardustxr/Controller/left",
	)
	.await
	.ok()?
	.import(client)
	.await?;
	let tracked = TrackedProxy::new(
		&Connection::session().await.ok()?,
		WellKnownName::from_static_str("org.stardustxr.Controllers").ok()?,
		"/org/stardustxr/Controller/left",
	)
	.await
	.ok()?;

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
	client: &Arc<ClientHandle>,
) -> Option<(SpatialRef, Transform, TrackedProxy<'static>)> {
	let anchor = stardust_xr_fusion::objects::interfaces::SpatialRefProxy::new(
		&Connection::session().await.ok()?,
		WellKnownName::from_static_str("org.stardustxr.Hands").ok()?,
		"/org/stardustxr/Hand/left/palm",
	)
	.await
	.ok()?
	.import(client)
	.await?;
	let tracked = TrackedProxy::new(
		&Connection::session().await.ok()?,
		WellKnownName::from_static_str("org.stardustxr.Hands").ok()?,
		"/org/stardustxr/Hand/left",
	)
	.await
	.ok()?;

	Some((
		anchor,
		Transform::from_translation([0.0, -0.02, 0.03]),
		tracked,
	))
}
