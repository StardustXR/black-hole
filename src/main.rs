pub mod black_hole;
pub mod minimize;

use color_eyre::eyre::Result;
use glam::Quat;
use minimize::MinimizeButton;
use stardust_xr_fusion::{
	client::Client,
	core::schemas::zbus::{names::WellKnownName, Connection},
	node::NodeType,
	objects::SpatialRefProxyExt,
	root::RootAspect,
	spatial::{SpatialRef, Transform},
};
use std::{
	f32::consts::{FRAC_PI_2, PI},
	sync::Arc,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let (client, event_loop) = Client::connect_with_async_loop()
		.await
		.expect("Unable to connect to server");

	let (anchor, offset) = controller_transform(&client).await?;
	// let (anchor, offset) = hand_transform(&client).await?;
	let root = MinimizeButton::new(&anchor, offset).await?;
	// let root = MinimizeButton::new(
	// 	client.get_root(),
	// 	Transform::from_translation([0.0, 0.0, -0.3]),
	// )
	// .await?;
	let _root_wrapper = client.get_root().alias().wrap(root)?;

	tokio::select! {
		e = tokio::signal::ctrl_c() => e?,
		e = event_loop => e??,
	};
	Ok(())
}

pub async fn controller_transform(client: &Arc<Client>) -> Result<(SpatialRef, Transform)> {
	let anchor = stardust_xr_fusion::objects::interfaces::SpatialRefProxy::new(
		&Connection::session().await?,
		WellKnownName::from_static_str("org.stardustxr.Controllers")?,
		"/org/stardustxr/Controller/left",
	)
	.await?
	.import(client)
	.await
	.unwrap();

	Ok((
		anchor,
		Transform::from_translation_rotation(
			[0.0, 0.01, 0.02],
			Quat::from_rotation_x(PI + FRAC_PI_2),
		),
	))
}
pub async fn hand_transform(client: &Arc<Client>) -> Result<(SpatialRef, Transform)> {
	let anchor = stardust_xr_fusion::objects::interfaces::SpatialRefProxy::new(
		&Connection::session().await?,
		WellKnownName::from_static_str("org.stardustxr.Hands")?,
		"/org/stardustxr/Hand/left/palm",
	)
	.await?
	.import(client)
	.await
	.unwrap();

	Ok((anchor, Transform::from_translation([0.0, -0.1, 0.0])))
}
