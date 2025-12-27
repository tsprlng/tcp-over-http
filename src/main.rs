//#![allow(warnings)]
#![cfg_attr(
    not(feature = "rustc_stable"),
    feature(
        core_intrinsics,
        auto_traits,
        negative_impls,
        panic_internals,
        panic_info_message
    )
)]
#![allow(clippy::needless_return)]
#![warn(clippy::pedantic)]

use anyhow::anyhow;
use clap::{Parser, Subcommand};
use reqwest::Url;
use std::{convert::Infallible, net::SocketAddr, str::FromStr};
use std::error::Error;
use tokio::net::lookup_host;
use halfbrown::HashMap as Map;

mod auth;

mod entry;
mod exit;

#[cfg(test)]
mod tests;

#[cfg(not(feature = "rustc_stable"))]
mod error;

pub(crate) fn join_url<'a>(base: &Url, path: impl IntoIterator<Item = &'a str>) -> Url {
    //url::ParseError
    assert!(base.path().ends_with('/'));
    let mut it = path.into_iter();
    let first = it.next().unwrap();
    let mut next = it.next();
    if next.is_some() {
        assert!(first.ends_with('/'));
    }
    let mut last = base.join(first).unwrap();
    while let Some(x) = next {
        last = last.join(x).unwrap();
        next = it.next();
        if next.is_some() {
            assert!(x.ends_with('/'));
        }
    }
    return last;
}

#[derive(Clone, Debug, Subcommand)]
enum CommandMode {
    /// Spin up entry node. Receives incoming TCP and forwards HTTP.
    Entry {
        #[clap(short, long, value_parser, default_value = "localhost:1415")]
        bind_addr: ResolveAddr,

        /// URL of the exit node.
        #[clap(short='u', long, value_parser)]
        target_url: Url,

        #[clap(short='t', long)]
        target_id: String,

        /// Auth token.
        #[clap(short, long, value_parser)]
        auth: String,
    },
    /// Spin up exit node. Receives incoming HTTP and forwards TCP.
    Exit {
        #[clap(short, long, value_parser, default_value = "localhost:8080")]
        bind_addr: ResolveAddr,

        #[arg(short = 't', long = "target", value_names = ["ID", "URL"], num_args = 2)]
        raw_target_addresses: Vec<String>,
    },
}

impl CommandMode {
    fn target_addresses(&self) -> Option<Map<String, ResolveAddr>> {
        match self {
            CommandMode::Exit { raw_target_addresses, .. } => {
                let mut target_map = Map::new();
                for chunk in raw_target_addresses.chunks(2) {
                    if chunk.len() == 2 {
                        target_map.insert(chunk[0].clone(), ResolveAddr(chunk[1].clone()));
                    }
                }
                Some(target_map)
            },
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct ResolveAddr(String); //lookup_host
impl FromStr for ResolveAddr {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}
impl ResolveAddr {
    async fn resolve(self) -> Vec<SocketAddr> {
        lookup_host(&self.0)
            .await
            .map_err(|e| anyhow!("{self:#?} - {e:#?}"))
            .unwrap()
            .collect::<Vec<_>>()
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct CliArgs {
    #[clap(subcommand)]
    pub mode: CommandMode,
}

fn init_panic_hook() {
    static ONCE_GUARD: std::sync::Once = std::sync::Once::new();
    ONCE_GUARD.call_once(|| {
        let org = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            org(info);
            std::process::exit(101);
        }));
    });
}

#[tokio::main]
async fn main() {
    init_panic_hook();

    let mode = CliArgs::parse().mode;
    match mode {
        CommandMode::Entry {
            bind_addr,
            target_url,
            auth,
            target_id,
        } => {
            entry::main(&bind_addr.resolve().await, target_url, auth, target_id)
                .await
                .1
                .await;
        }
        CommandMode::Exit {
            ref bind_addr,
            ref raw_target_addresses,
        } => {
            let resolve_addresses = mode.target_addresses().unwrap();
            let mut target_addresses = Map::new();
            for (id, addr) in resolve_addresses {
                target_addresses.insert(id, addr.resolve().await);
            }
            exit::main(&bind_addr.clone().resolve().await, target_addresses)
                .1
                .await
                .unwrap();
        }
    }
}

use std::pin::Pin;
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::OwnedMutexGuard;
use tokio_util::codec::{BytesCodec, FramedRead};

#[ouroboros::self_referencing]
pub(crate) struct Wrapper<T: 'static> {
    guard: OwnedMutexGuard<T>,
    #[borrows(mut guard)]
    #[not_covariant]
    fr: FramedRead<&'this mut OwnedReadHalf, BytesCodec>,
}

impl<T: 'static> futures::Stream for Wrapper<T> {
    type Item = Result<bytes::BytesMut, std::io::Error>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.with_fr_mut(|fr| Pin::new(fr).poll_next(cx))
    }
}

pub(crate) type Artex<T> = std::sync::Arc<tokio::sync::Mutex<T>>;
pub(crate) fn artex<T>(t: T) -> Artex<T> {
    std::sync::Arc::new(tokio::sync::Mutex::new(t))
}
