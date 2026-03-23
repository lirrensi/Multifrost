use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures_util::stream::SplitSink;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::error::{Result, RouterError};
use crate::protocol::PeerClass;

pub type WebSocketWrite = SplitSink<WebSocketStream<TcpStream>, Message>;
pub type PeerSender = Arc<Mutex<WebSocketWrite>>;

#[derive(Debug, Clone)]
pub struct PeerSnapshot {
    pub peer_id: String,
    pub class: PeerClass,
    pub connected: bool,
}

#[derive(Debug, Clone)]
struct PeerRecord {
    peer_id: String,
    class: PeerClass,
    sender: PeerSender,
    connected: bool,
    last_heartbeat: Option<Instant>,
}

#[derive(Debug, Default, Clone)]
pub struct PeerRegistry {
    peers: Arc<RwLock<HashMap<String, PeerRecord>>>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert_live_peer(
        &self,
        peer_id: String,
        class: PeerClass,
        sender: PeerSender,
    ) -> Result<()> {
        let mut peers = self.peers.write().await;
        if peers.contains_key(&peer_id) {
            return Err(RouterError::DuplicatePeerId(peer_id));
        }

        peers.insert(
            peer_id.clone(),
            PeerRecord {
                peer_id,
                class,
                sender,
                connected: true,
                last_heartbeat: Some(Instant::now()),
            },
        );
        Ok(())
    }

    pub async fn reject_duplicate_live_peer_id(&self, peer_id: &str) -> bool {
        self.peers.read().await.contains_key(peer_id)
    }

    pub async fn lookup_by_peer_id(&self, peer_id: &str) -> Option<PeerSnapshot> {
        self.peers
            .read()
            .await
            .get(peer_id)
            .map(|record| PeerSnapshot {
                peer_id: record.peer_id.clone(),
                class: record.class.clone(),
                connected: record.connected,
            })
    }

    pub async fn sender_by_peer_id(&self, peer_id: &str) -> Option<PeerSender> {
        self.peers
            .read()
            .await
            .get(peer_id)
            .map(|record| record.sender.clone())
    }

    pub async fn remove_by_peer_id(&self, peer_id: &str) -> Option<PeerSnapshot> {
        self.peers
            .write()
            .await
            .remove(peer_id)
            .map(|record| PeerSnapshot {
                peer_id: record.peer_id,
                class: record.class,
                connected: false,
            })
    }

    pub async fn snapshot_peer_exists(&self, peer_id: &str) -> bool {
        self.peers.read().await.contains_key(peer_id)
    }

    pub async fn snapshot_peer_class(&self, peer_id: &str) -> Option<PeerClass> {
        self.peers
            .read()
            .await
            .get(peer_id)
            .map(|record| record.class.clone())
    }

    pub async fn mark_heartbeat(&self, peer_id: &str) {
        if let Some(record) = self.peers.write().await.get_mut(peer_id) {
            record.last_heartbeat = Some(Instant::now());
        }
    }

    pub async fn connected_snapshot(&self, peer_id: &str) -> Option<bool> {
        self.peers
            .read()
            .await
            .get(peer_id)
            .map(|record| record.connected)
    }
}
