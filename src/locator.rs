use glam::Vec3;
use stardust_xr_asteroids::{Context, CreateInnerInfo, CustomElement, FnWrapper, ValidState};
use stardust_xr_fusion::{
	client::FrameInfo,
	spatial::{Spatial, SpatialRef},
};
use std::fmt::Debug;
use tokio::sync::oneshot::{self, error::TryRecvError};

type OnLocated<State> = FnWrapper<dyn Fn(&mut State, Vec3) + Send + Sync>;

/// finds where it sits in the client's root space, once each time `sample` changes to something new
#[derive(Debug)]
pub struct Locator<State: ValidState + Debug> {
	sample: Option<u32>,
	on_located: OnLocated<State>,
}
impl<State: ValidState + Debug> Locator<State> {
	pub fn new(
		sample: Option<u32>,
		on_located: impl Fn(&mut State, Vec3) + Send + Sync + 'static,
	) -> Self {
		Locator {
			sample,
			on_located: FnWrapper(Box::new(on_located)),
		}
	}
}
impl<State: ValidState + Debug> CustomElement<State> for Locator<State> {
	type Inner = LocatorInner;
	type Error = stardust_xr_fusion::Error;

	async fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
	) -> Result<LocatorInner, Self::Error> {
		Ok(LocatorInner {
			space: info.child_space.spatial_ref().await?,
			_spatial: info.child_space,
			located: None,
		})
	}

	fn diff(&self, old_self: &Self, context: &Context, inner: &mut LocatorInner) {
		if self.sample.is_none() || self.sample == old_self.sample {
			return;
		}
		let (tx, rx) = oneshot::channel();
		let client = context.stardust_client.clone();
		let space = inner.space.clone();
		tokio::spawn(async move {
			if let Ok(Ok(t)) = client
				.spatial_interface()
				.get_relative_transform(client.root().clone(), space)
				.await
			{
				let _ = tx.send(t.translation.into());
			}
		});
		inner.located = Some(rx);
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inner: &mut LocatorInner,
	) {
		let Some(rx) = &mut inner.located else {
			return;
		};
		match rx.try_recv() {
			Ok(p) => (self.on_located.0)(state, p),
			Err(TryRecvError::Empty) => return,
			Err(TryRecvError::Closed) => (),
		}
		inner.located = None;
	}
}

pub struct LocatorInner {
	_spatial: Spatial,
	space: SpatialRef,
	located: Option<oneshot::Receiver<Vec3>>,
}
