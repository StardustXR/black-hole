pub mod black_hole;
pub mod minimize;

use black_hole::BlackHole;
use color_eyre::eyre::Result;
use glam::Quat;
use manifest_dir_macros::directory_relative_path;
use minimize::MinimizeButton;
use stardust_xr_fusion::{
	client::Client,
	core::schemas::zbus::{names::WellKnownName, Connection},
	objects::SpatialRefProxyExt,
	root::{RootAspect, RootEvent},
	spatial::{SpatialRef, Transform},
	ClientHandle,
};
use std::{
	f32::consts::{FRAC_PI_2, PI},
	sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let client = Client::connect()
		.await
		.expect("Unable to connect to server");
	let client_handle = client.handle();
	client.async_event_loop();
	client_handle
		.get_root()
		.set_base_prefixes(&[directory_relative_path!("res").to_owned()])?;

	let black_hole = BlackHole::new(client_handle.get_root())?;
	let mut buttons: [Option<MinimizeButton>; 2] = [None, None];
	let mut was_spawned = false;
	if let Some((anchor, offset)) = controller_transform(&client_handle).await {
		was_spawned = true;
		buttons[0] = Some(MinimizeButton::new(&anchor, offset).await?);
	};
	if let Some((anchor, offset)) = hand_transform(&client_handle).await {
		was_spawned = true;
		buttons[1] = Some(MinimizeButton::new(&anchor, offset).await?);
	}
	if !was_spawned {
		println!("hitting the fucking fallback!");
		buttons[0] = Some(
			MinimizeButton::new(
				client_handle.get_root(),
				Transform::from_translation([0.0, 0.0, -0.3]),
			)
			.await?,
		);
	};
	let _main_loop = std::thread::spawn(move || main_loop(&client_handle, black_hole, buttons));

	tokio::signal::ctrl_c().await?;
	Ok(())
}

fn main_loop(
	client: &Arc<ClientHandle>,
	mut black_hole: BlackHole,
	mut buttons: [Option<MinimizeButton>; 2],
) {
	loop {
		let Some(event) = client.get_root().recv_root_event() else {
			continue;
		};
		match event {
			RootEvent::Frame { info } => {
				black_hole.frame(&info);
				for button in buttons.iter_mut().filter_map(Option::as_mut) {
					button.frame(&mut black_hole);
				}
			}
			RootEvent::SaveState { response: _ } => {}
		}
	}
}

pub async fn controller_transform(client: &Arc<ClientHandle>) -> Option<(SpatialRef, Transform)> {
	let anchor = stardust_xr_fusion::objects::interfaces::SpatialRefProxy::new(
		&Connection::session().await.ok()?,
		WellKnownName::from_static_str("org.stardustxr.Controllers").ok()?,
		"/org/stardustxr/Controller/left",
	)
	.await
	.ok()?
	.import(client)
	.await?;

	Some((
		anchor,
		Transform::from_translation_rotation(
			[0.0, 0.01, 0.02],
			Quat::from_rotation_x(PI + FRAC_PI_2),
		),
	))
}
pub async fn hand_transform(client: &Arc<ClientHandle>) -> Option<(SpatialRef, Transform)> {
	let anchor = stardust_xr_fusion::objects::interfaces::SpatialRefProxy::new(
		&Connection::session().await.ok()?,
		WellKnownName::from_static_str("org.stardustxr.Hands").ok()?,
		"/org/stardustxr/Hand/left/palm",
	)
	.await
	.ok()?
	.import(client)
	.await?;

	Some((anchor, Transform::from_translation([0.0, -0.03, -0.06])))
}
