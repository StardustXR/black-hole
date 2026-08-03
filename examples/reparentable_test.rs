//! Minimal, self-contained known-working case for the Reparentable + points_query
//! pairing: this one process registers both a dummy object that implements
//! Reparentable and a points_query looking for it, right on top of each other at
//! root. If ENTERED never shows up here, the query/registration pairing itself is
//! broken. If it does, the bug is specific to black-hole's own setup (or to
//! whatever external object you're testing against).
//!
//! Run with: cargo run --example reparentable_test

use gluon::Handler;
use stardust_xr_fusion::{
	client::Client,
	fields::{Field, FieldExt, FieldRef, FieldSample, Shape},
	project_local_resources,
	query::{InterfaceDependency, QueriedInterface, QueryableObjectRef},
	spatial::{Spatial, SpatialExt, SpatialRef, Transform},
	spatial_query::{Point, PointsQuery, PointsQueryHandler, PointsQueryHandlerHandler},
};
use stardust_xr_molecules::reparentable::{Reparentable};
use tokio::sync::broadcast::error::RecvError;

#[derive(Debug, Handler)]
struct Logger;
impl PointsQueryHandlerHandler for Logger {
	async fn entered(
		&self,
		_ctx: gluon::Context,
		obj: QueryableObjectRef,
		_field: FieldRef,
		_spatial: SpatialRef,
		interfaces: Vec<QueriedInterface>,
		_spatial_info: FieldSample,
	) {
		tracing::info!(?obj, ?interfaces, "ENTERED");
	}
	async fn interfaces_changed(
		&self,
		_ctx: gluon::Context,
		obj: QueryableObjectRef,
		interfaces: Vec<QueriedInterface>,
	) {
		tracing::info!(?obj, ?interfaces, "INTERFACES CHANGED");
	}
	async fn moved(
		&self,
		_ctx: gluon::Context,
		_obj: QueryableObjectRef,
		_spatial_info: FieldSample,
	) {
	}
	async fn left(&self, _ctx: gluon::Context, obj: QueryableObjectRef) {
		tracing::info!(?obj, "LEFT");
	}
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt().pretty().with_file(false).init();

	let (client, root) = Client::auto_connect(&[&project_local_resources!("res")])
		.await
		.expect("Unable to connect to server");

	// provider: a dummy object that registers itself as Reparentable, sitting at root
	let (dummy_spatial, dummy_ref) = Spatial::new(&client, &root, Transform::IDENTITY)
		.await
		.expect("failed to create dummy spatial");
	let (dummy_field, _) = Field::new(&client, &dummy_spatial, Shape::Sphere { radius: 0.1 })
		.await
		.expect("failed to create dummy field");
	let _reparentable = Reparentable::new(&client, dummy_spatial, dummy_ref, dummy_field)
		.await
		.expect("failed to register dummy reparentable");
	tracing::info!("dummy reparentable object registered at root");

	// consumer: a points query with a single point at root, looking for Reparentable objects
	let handler = client.pion_device().register_object(Logger);
	let _query = client
		.spatial_query_interface()
		.points_query(PointsQuery {
			handler: PointsQueryHandler::from_handler(&handler),
			interfaces: vec![InterfaceDependency {
				id: REPARENTABLE_PROTOCOL.protocol_name.into(),
				optional: false,
			}],
			reference_spatial: root.clone(),
			points: vec![Point {
				point: [0.0, 0.0, 0.0].into(),
				margin: 1.0,
			}],
		})
		.await
		.expect("failed to send points_query")
		.expect("points_query rejected");
	tracing::info!("points query registered, watching for reparentable objects");

	let mut recv = client.frame_receiver();
	loop {
		match recv.recv().await {
			Ok(_) => {}
			Err(RecvError::Closed) => break,
			Err(RecvError::Lagged(_)) => continue,
		}
	}
}
