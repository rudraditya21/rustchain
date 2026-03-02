#![allow(dead_code)]

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::blockchain::chain::Blockchain;
use crate::blockchain::error::BlockchainError;
use crate::core::hash::Hash32;
use crate::core::transaction::Transaction;
use crate::network::error::NetworkError;
use crate::network::protocol::ProtocolMessage;

pub struct P2pNode {
    listen_addr: SocketAddr,
    peers: RwLock<Vec<SocketAddr>>,
    blockchain: Arc<Mutex<Blockchain>>,
    seen_block_hashes: Mutex<HashSet<[u8; 32]>>,
    seen_tx_hashes: Mutex<HashSet<[u8; 32]>>,
    shutdown_tx: broadcast::Sender<()>,
    accept_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl P2pNode {
    pub async fn start(
        bind_addr: &str,
        peers: Vec<String>,
        blockchain: Arc<Mutex<Blockchain>>,
    ) -> Result<Arc<Self>, NetworkError> {
        let peer_addrs = parse_peer_list(&peers)?;
        let listener = TcpListener::bind(bind_addr).await?;
        let local_addr = listener.local_addr()?;

        let (shutdown_tx, _) = broadcast::channel(1);
        let node = Arc::new(Self {
            listen_addr: local_addr,
            peers: RwLock::new(peer_addrs),
            blockchain,
            seen_block_hashes: Mutex::new(HashSet::new()),
            seen_tx_hashes: Mutex::new(HashSet::new()),
            shutdown_tx,
            accept_handle: Mutex::new(None),
        });

        let accept_node = Arc::clone(&node);
        let mut shutdown_rx = accept_node.shutdown_tx.subscribe();
        let accept_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                    accept_result = listener.accept() => {
                        let Ok((stream, _remote)) = accept_result else {
                            continue;
                        };
                        let connection_node = Arc::clone(&accept_node);
                        tokio::spawn(async move {
                            let _ = connection_node.handle_connection(stream).await;
                        });
                    }
                }
            }
        });

        *node.accept_handle.lock().await = Some(accept_handle);
        let _ = node.sync_with_peers().await;
        Ok(node)
    }

    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    pub async fn set_peers(&self, peers: Vec<String>) -> Result<(), NetworkError> {
        let parsed = parse_peer_list(&peers)?;
        *self.peers.write().await = parsed;
        Ok(())
    }

    pub async fn submit_transaction(&self, tx: Transaction) -> Result<Hash32, NetworkError> {
        let tx_hash = {
            let mut chain = self.blockchain.lock().await;
            chain.admit_transaction(tx.clone())?
        };

        if self.mark_tx_seen(&tx_hash).await {
            self.broadcast_to_peers(ProtocolMessage::NewTransaction { tx })
                .await;
        }
        Ok(tx_hash)
    }

    pub async fn mine_next_block_and_broadcast(
        &self,
        timestamp_unix: u64,
        max_nonce: u64,
    ) -> Result<Hash32, NetworkError> {
        let (block_hash, block) = {
            let mut chain = self.blockchain.lock().await;
            let candidate = chain.build_candidate_block(timestamp_unix);
            let mined = chain.mine_candidate_block(candidate, max_nonce)?;
            let hash = chain.apply_block(mined.clone())?;
            (hash, mined)
        };

        if self.mark_block_seen(&block_hash).await {
            self.broadcast_to_peers(ProtocolMessage::NewBlock { block })
                .await;
        }
        Ok(block_hash)
    }

    pub async fn sync_with_peers(&self) -> Result<(), NetworkError> {
        let peers = self.peers.read().await.clone();
        for peer in peers {
            if let Ok(Some(chain_blocks)) = self.request_chain_from_peer(peer).await {
                let mut chain = self.blockchain.lock().await;
                let _ = chain.consider_fork(chain_blocks);
            }
        }
        Ok(())
    }

    pub async fn chain_height(&self) -> u64 {
        let chain = self.blockchain.lock().await;
        chain.chain_height()
    }

    pub async fn tip_hash(&self) -> Hash32 {
        let chain = self.blockchain.lock().await;
        chain.tip_hash()
    }

    pub async fn mempool_len(&self) -> usize {
        let chain = self.blockchain.lock().await;
        chain.mempool_len()
    }

    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.accept_handle.lock().await.take() {
            let _ = handle.await;
        }
    }

    async fn handle_connection(&self, mut stream: TcpStream) -> Result<(), NetworkError> {
        let message = read_message(&mut stream).await?;
        match message {
            ProtocolMessage::GetChain => {
                let blocks = {
                    let chain = self.blockchain.lock().await;
                    chain.blocks()
                };
                write_message(&mut stream, &ProtocolMessage::ChainResponse { blocks }).await?;
            }
            ProtocolMessage::ChainResponse { blocks } => {
                let mut chain = self.blockchain.lock().await;
                let _ = chain.consider_fork(blocks);
            }
            ProtocolMessage::NewTransaction { tx } => {
                let tx_hash = tx.tx_hash();
                if self.is_tx_seen(&tx_hash).await {
                    return Ok(());
                }

                let admitted = {
                    let mut chain = self.blockchain.lock().await;
                    chain.admit_transaction(tx.clone())
                };

                if admitted.is_ok() && self.mark_tx_seen(&tx_hash).await {
                    self.broadcast_to_peers(ProtocolMessage::NewTransaction { tx })
                        .await;
                }
            }
            ProtocolMessage::NewBlock { block } => {
                let block_hash = block.hash();
                if self.is_block_seen(&block_hash).await {
                    return Ok(());
                }

                let apply_result = {
                    let mut chain = self.blockchain.lock().await;
                    chain.apply_block(block.clone())
                };

                match apply_result {
                    Ok(applied_hash) => {
                        if self.mark_block_seen(&applied_hash).await {
                            self.broadcast_to_peers(ProtocolMessage::NewBlock { block })
                                .await;
                        }
                    }
                    Err(BlockchainError::InvalidPreviousHash { .. }) => {
                        let _ = self.sync_with_peers().await;
                    }
                    Err(_) => {}
                }
            }
        }
        Ok(())
    }

    async fn request_chain_from_peer(
        &self,
        peer: SocketAddr,
    ) -> Result<Option<Vec<crate::core::block::Block>>, NetworkError> {
        let mut stream = TcpStream::connect(peer).await?;
        write_message(&mut stream, &ProtocolMessage::GetChain).await?;
        let response = read_message(&mut stream).await?;
        match response {
            ProtocolMessage::ChainResponse { blocks } => Ok(Some(blocks)),
            _ => Err(NetworkError::UnexpectedMessage(
                "expected ChainResponse for GetChain",
            )),
        }
    }

    async fn broadcast_to_peers(&self, message: ProtocolMessage) {
        let peers = self.peers.read().await.clone();
        for peer in peers {
            if peer == self.listen_addr {
                continue;
            }
            let outbound = message.clone();
            tokio::spawn(async move {
                if let Ok(mut stream) = TcpStream::connect(peer).await {
                    let _ = write_message(&mut stream, &outbound).await;
                }
            });
        }
    }

    async fn mark_block_seen(&self, hash: &Hash32) -> bool {
        let mut seen = self.seen_block_hashes.lock().await;
        seen.insert(hash.0)
    }

    async fn is_block_seen(&self, hash: &Hash32) -> bool {
        let seen = self.seen_block_hashes.lock().await;
        seen.contains(&hash.0)
    }

    async fn mark_tx_seen(&self, hash: &Hash32) -> bool {
        let mut seen = self.seen_tx_hashes.lock().await;
        seen.insert(hash.0)
    }

    async fn is_tx_seen(&self, hash: &Hash32) -> bool {
        let seen = self.seen_tx_hashes.lock().await;
        seen.contains(&hash.0)
    }
}

async fn write_message(
    stream: &mut TcpStream,
    message: &ProtocolMessage,
) -> Result<(), NetworkError> {
    let payload = serde_json::to_vec(message)?;
    if payload.len() > u32::MAX as usize {
        return Err(NetworkError::FrameTooLarge(payload.len()));
    }

    stream.write_u32(payload.len() as u32).await?;
    stream.write_all(&payload).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_message(stream: &mut TcpStream) -> Result<ProtocolMessage, NetworkError> {
    let len = stream.read_u32().await?;
    let mut payload = vec![0u8; len as usize];
    stream.read_exact(&mut payload).await?;
    Ok(serde_json::from_slice(&payload)?)
}

fn parse_peer_list(peers: &[String]) -> Result<Vec<SocketAddr>, NetworkError> {
    peers
        .iter()
        .map(|peer| peer.parse::<SocketAddr>().map_err(NetworkError::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::tempdir;
    use tokio::sync::Mutex;
    use tokio::time::{sleep, Duration};

    use crate::blockchain::chain::{Blockchain, ChainConfig};
    use crate::blockchain::error::BlockchainError;
    use crate::blockchain::state::GenesisAccount;
    use crate::core::hash::Hash32;
    use crate::core::transaction::{SignedTransactionPayload, Transaction};
    use crate::crypto::signature::SecretKeyBytes;
    use crate::crypto::wallet::Wallet;
    use crate::network::error::NetworkError;
    use crate::network::p2p::P2pNode;

    fn chain_config() -> ChainConfig {
        ChainConfig {
            difficulty_bits: 0,
            max_transactions_per_block: 1_000,
            genesis_timestamp_unix: 1_700_010_000,
        }
    }

    fn wallets_and_genesis() -> (Wallet, Wallet, Vec<GenesisAccount>) {
        let wallet_a = Wallet::from_secret_key(SecretKeyBytes([31u8; 32]));
        let wallet_b = Wallet::from_secret_key(SecretKeyBytes([32u8; 32]));
        let genesis = vec![
            GenesisAccount::from_public_key(&wallet_a.public_key_bytes(), 20_000),
            GenesisAccount::from_public_key(&wallet_b.public_key_bytes(), 2_000),
        ];
        (wallet_a, wallet_b, genesis)
    }

    fn signed_tx(wallet: &Wallet, to: String, amount: u64, fee: u64, nonce: u64) -> Transaction {
        let payload = SignedTransactionPayload {
            from: wallet.public_key_hex(),
            to,
            amount,
            fee,
            nonce,
        };
        let signature = wallet.sign_payload(&payload);

        Transaction {
            from: payload.from,
            to: payload.to,
            amount,
            fee,
            nonce,
            signature: signature.0.to_vec(),
        }
    }

    async fn wait_for_condition(
        timeout: Duration,
        condition: impl Fn() -> tokio::task::JoinHandle<bool>,
    ) -> bool {
        let start = tokio::time::Instant::now();
        loop {
            if condition().await.unwrap_or(false) {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tx_and_block_propagate_between_nodes() -> Result<(), NetworkError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir_a = tempdir()?;
        let dir_b = tempdir()?;

        let chain_a = Blockchain::open_or_init(dir_a.path(), chain_config(), genesis.clone())?;
        let chain_b = Blockchain::open_or_init(dir_b.path(), chain_config(), genesis)?;

        let node_a = P2pNode::start("127.0.0.1:0", vec![], Arc::new(Mutex::new(chain_a))).await?;
        let node_b = P2pNode::start("127.0.0.1:0", vec![], Arc::new(Mutex::new(chain_b))).await?;

        node_a
            .set_peers(vec![node_b.listen_addr().to_string()])
            .await?;
        node_b
            .set_peers(vec![node_a.listen_addr().to_string()])
            .await?;

        let tx = signed_tx(&wallet_a, wallet_b.address(), 10, 1, 1);
        node_a.submit_transaction(tx).await?;

        let mempool_propagated = wait_for_condition(Duration::from_secs(3), || {
            let node_b = Arc::clone(&node_b);
            tokio::spawn(async move { node_b.mempool_len().await == 1 })
        })
        .await;
        assert!(mempool_propagated);

        node_a
            .mine_next_block_and_broadcast(1_700_010_100, 0)
            .await?;

        let tips_converged = wait_for_condition(Duration::from_secs(5), || {
            let node_a = Arc::clone(&node_a);
            let node_b = Arc::clone(&node_b);
            tokio::spawn(async move {
                let tip_a = node_a.tip_hash().await;
                let tip_b = node_b.tip_hash().await;
                tip_a == tip_b && tip_a != Hash32::ZERO && node_b.chain_height().await == 1
            })
        })
        .await;
        assert!(tips_converged);

        node_a.shutdown().await;
        node_b.shutdown().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lagging_node_catches_up_via_sync() -> Result<(), NetworkError> {
        let (wallet_a, wallet_b, genesis) = wallets_and_genesis();
        let dir_a = tempdir()?;
        let dir_b = tempdir()?;

        let chain_a = Blockchain::open_or_init(dir_a.path(), chain_config(), genesis.clone())?;
        let chain_b = Blockchain::open_or_init(dir_b.path(), chain_config(), genesis)?;

        let node_a = P2pNode::start("127.0.0.1:0", vec![], Arc::new(Mutex::new(chain_a))).await?;

        for nonce in 1..=3 {
            let tx = signed_tx(&wallet_a, wallet_b.address(), 4, 1, nonce);
            node_a.submit_transaction(tx).await?;
            node_a
                .mine_next_block_and_broadcast(1_700_010_200 + nonce, 0)
                .await?;
        }

        let node_b = P2pNode::start(
            "127.0.0.1:0",
            vec![node_a.listen_addr().to_string()],
            Arc::new(Mutex::new(chain_b)),
        )
        .await?;
        node_b.sync_with_peers().await?;

        let caught_up = wait_for_condition(Duration::from_secs(5), || {
            let node_a = Arc::clone(&node_a);
            let node_b = Arc::clone(&node_b);
            tokio::spawn(async move {
                node_a.tip_hash().await == node_b.tip_hash().await
                    && node_a.chain_height().await == node_b.chain_height().await
                    && node_b.chain_height().await >= 3
            })
        })
        .await;
        assert!(caught_up);

        node_a.shutdown().await;
        node_b.shutdown().await;
        Ok(())
    }

    #[test]
    fn network_error_from_blockchain_error_compiles() {
        let error = NetworkError::from(BlockchainError::EmptyChain);
        assert!(matches!(error, NetworkError::Blockchain(_)));
    }
}
