use chrono::{NaiveDateTime, Utc};
use log::{debug, error};
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        schnorr::{SchnorrPublic, SchnorrSecret},
    },
    util::serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable},
    Error, Result,
};
use rand::rngs::OsRng;

use super::{
    block::{proposal_eq_block, Block, BlockProposal},
    blockchain::Blockchain,
    participant::Participant,
    tx::Tx,
    util::Timestamp,
    vote::Vote,
};

const DELTA: u64 = 60;
const SLED_STATE_TREE: &[u8] = b"_state";

/// Atomic pointer to state.
pub type StatePtr = Arc<RwLock<State>>;

/// This struct represents the state of a consensus node.
/// Each node is numbered and has a secret-public keys pair, to sign messages.
/// Nodes hold a set of Blockchains(some of which are not notarized)
/// and a set of unconfirmed pending transactions.
#[derive(SerialEncodable, SerialDecodable)]
pub struct State {
    pub id: u64,
    pub genesis_time: Timestamp,
    pub secret_key: SecretKey,
    pub public_key: PublicKey,
    pub canonical_blockchain: Blockchain,
    pub node_blockchains: Vec<Blockchain>,
    pub unconfirmed_txs: Vec<Tx>,
    pub orphan_votes: Vec<Vote>,
    pub participants: BTreeMap<u64, Participant>,
    pub pending_participants: Vec<Participant>,
}

impl State {
    pub fn new(id: u64, genesis_time: Timestamp, init_block: Block) -> State {
        // TODO: clock sync
        let secret = SecretKey::random(&mut OsRng);
        State {
            id,
            genesis_time,
            secret_key: secret,
            public_key: PublicKey::from_secret(secret),
            canonical_blockchain: Blockchain::new(init_block),
            node_blockchains: Vec::new(),
            unconfirmed_txs: Vec::new(),
            orphan_votes: Vec::new(),
            participants: BTreeMap::new(),
            pending_participants: Vec::new(),
        }
    }

    /// Node retreives a transaction and append it to the unconfirmed transactions list.
    /// Additional validity rules must be defined by the protocol for transactions.
    pub fn append_tx(&mut self, tx: Tx) -> bool {
        if self.unconfirmed_txs.contains(&tx) {
            return false
        }
        self.unconfirmed_txs.push(tx);
        true
    }

    /// Node calculates seconds until next epoch starting time.
    /// Epochs duration is configured using the delta value.
    pub fn get_seconds_until_next_epoch_start(&self) -> Duration {
        let start_time = NaiveDateTime::from_timestamp(self.genesis_time.0, 0);
        let current_epoch = self.get_current_epoch() + 1;
        let next_epoch_start_timestamp =
            (current_epoch * (2 * DELTA)) + (start_time.timestamp() as u64);
        let next_epoch_start =
            NaiveDateTime::from_timestamp(next_epoch_start_timestamp.try_into().unwrap(), 0);
        let current_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
        let diff = next_epoch_start - current_time;
        Duration::new(diff.num_seconds().try_into().unwrap(), 0)
    }

    /// Node calculates current epoch, based on elapsed time from the genesis block.
    /// Epochs duration is configured using the delta value.
    pub fn get_current_epoch(&self) -> u64 {
        self.genesis_time.clone().elapsed() / (2 * DELTA)
    }

    /// Node finds epochs leader, using a simple hash method.
    /// Leader calculation is based on how many nodes are participating in the network.
    pub fn get_epoch_leader(&mut self) -> u64 {
        let epoch = self.get_current_epoch();
        let mut hasher = DefaultHasher::new();
        epoch.hash(&mut hasher);
        self.zero_participants_check();
        let pos = hasher.finish() % (self.participants.len() as u64);
        self.participants.iter().nth(pos as usize).unwrap().1.id
    }

    /// Node checks if they are the current epoch leader.
    pub fn check_if_epoch_leader(&mut self) -> bool {
        let leader = self.get_epoch_leader();
        self.id == leader
    }

    /// Node generates a block proposal for the current epoch,
    /// containing all uncorfirmed transactions.
    /// Block extends the longest notarized blockchain the node holds.
    pub fn propose_block(&self) -> Result<Option<BlockProposal>> {
        let epoch = self.get_current_epoch();
        let longest_notarized_chain = self.find_longest_notarized_chain();
        let mut hasher = DefaultHasher::new();
        longest_notarized_chain.blocks.last().unwrap().hash(&mut hasher);
        let hash = hasher.finish().to_string();
        let unproposed_txs = self.get_unproposed_txs();
        let mut encoded_block = vec![];
        hash.encode(&mut encoded_block)?;
        epoch.encode(&mut encoded_block)?;
        unproposed_txs.encode(&mut encoded_block)?;
        let signed_block = self.secret_key.sign(&encoded_block[..]);
        Ok(Some(BlockProposal::new(
            self.public_key,
            signed_block,
            self.id,
            hash,
            epoch,
            unproposed_txs,
        )))
    }

    /// Node retrieves all unconfiremd transactions not proposed in previous blocks.
    pub fn get_unproposed_txs(&self) -> Vec<Tx> {
        let mut unproposed_txs = self.unconfirmed_txs.clone();
        for blockchain in &self.node_blockchains {
            for block in &blockchain.blocks {
                for tx in &block.txs {
                    if let Some(pos) = unproposed_txs.iter().position(|txs| *txs == *tx) {
                        unproposed_txs.remove(pos);
                    }
                }
            }
        }
        unproposed_txs
    }

    /// Finds the longest fully notarized blockchain the node holds.
    pub fn find_longest_notarized_chain(&self) -> &Blockchain {
        let mut longest_notarized_chain = &self.canonical_blockchain;
        let mut length = 0;
        for blockchain in &self.node_blockchains {
            if blockchain.is_notarized() && blockchain.blocks.len() > length {
                length = blockchain.blocks.len();
                longest_notarized_chain = blockchain;
            }
        }
        longest_notarized_chain
    }

    /// Node receives the proposed block, verifies its sender(epoch leader),
    /// and proceeds with voting on it.
    pub fn receive_proposed_block(
        &mut self,
        proposed_block: &BlockProposal,
        leader: bool,
    ) -> Result<Option<Vote>> {
        assert!(self.get_epoch_leader() == proposed_block.id);
        let mut encoded_block = vec![];
        proposed_block.st.encode(&mut encoded_block)?;
        proposed_block.sl.encode(&mut encoded_block)?;
        proposed_block.txs.encode(&mut encoded_block)?;
        assert!(proposed_block.public_key.verify(&encoded_block[..], &proposed_block.signature));
        self.vote_block(proposed_block, leader)
    }

    /// Given a block, node finds which blockchain it extends.
    /// If block extends the canonical blockchain, a new fork blockchain is created.
    /// Node votes on the block, only if it extends the longest notarized chain it has seen.
    pub fn vote_block(&mut self, proposal: &BlockProposal, leader: bool) -> Result<Option<Vote>> {
        self.zero_participants_check();
        let mut block = Block::new(
            proposal.st.clone(),
            proposal.sl,
            proposal.txs.clone(),
            String::from("proof"),
            String::from("r"),
            String::from("s"),
            self.participants.values().cloned().collect(),
        );

        // Add orphan votes
        let mut orphans = Vec::new();
        for (index, vote) in self.orphan_votes.iter().enumerate() {
            if proposal_eq_block(&vote.block, &block) {
                block.metadata.sm.votes.push(vote.clone());
                orphans.push(index);
            }
        }
        for index in orphans {
            self.orphan_votes.remove(index);
        }

        let index = self.find_extended_blockchain_index(&block, leader);

        if index == -2 {
            return Ok(None)
        }
        let blockchain = match index {
            -1 => {
                let blockchain = Blockchain::new(block);
                self.node_blockchains.push(blockchain);
                self.node_blockchains.last().unwrap()
            }
            _ => {
                self.node_blockchains[index as usize].add_block(&block);
                &self.node_blockchains[index as usize]
            }
        };

        if self.extends_notarized_blockchain(blockchain) {
            let mut encoded_proposal = vec![];
            proposal.encode(&mut encoded_proposal)?;
            let signed_proposal = self.secret_key.sign(&encoded_proposal[..]);
            return Ok(Some(Vote::new(self.public_key, signed_proposal, proposal.clone(), self.id)))
        }
        Ok(None)
    }

    /// Node verifies if provided blockchain is notarized excluding the last block.
    pub fn extends_notarized_blockchain(&self, blockchain: &Blockchain) -> bool {
        for block in &blockchain.blocks[..(blockchain.blocks.len() - 1)] {
            if !block.metadata.sm.notarized {
                return false
            }
        }
        true
    }

    /// Given a block, node finds the index of the blockchain it extends.
    pub fn find_extended_blockchain_index(&self, block: &Block, leader: bool) -> i64 {
        let mut hasher = DefaultHasher::new();
        for (index, blockchain) in self.node_blockchains.iter().enumerate() {
            let last_block = blockchain.blocks.last().unwrap();
            last_block.hash(&mut hasher);
            if (leader && block.st == hasher.finish().to_string() && block.sl >= last_block.sl) ||
                (!leader && block.st == hasher.finish().to_string() && block.sl > last_block.sl)
            {
                return index as i64
            }
        }

        let last_block = self.canonical_blockchain.blocks.last().unwrap();
        last_block.hash(&mut hasher);
        if (leader && block.st != hasher.finish().to_string() || block.sl < last_block.sl) ||
            (!leader && block.st != hasher.finish().to_string() || block.sl <= last_block.sl)
        {
            debug!("Proposed block doesn't extend any known chains.");
            return -2
        }
        -1
    }

    /// Node receives a vote for a block.
    /// First, sender is verified using their public key.
    /// Block is searched in nodes blockchains.
    /// If the vote wasn't received before, it is appended to block votes list.
    /// When a node sees 2n/3 votes for a block it notarizes it.
    /// When a block gets notarized, the transactions it contains are removed from
    /// nodes unconfirmed transactions list.
    /// Finally, we check if the notarization of the block can finalize parent blocks
    /// in its blockchain.
    pub fn receive_vote(&mut self, vote: &Vote) -> bool {
        let mut encoded_block = vec![];
        let result = vote.block.encode(&mut encoded_block);
        match result {
            Ok(_) => (),
            Err(e) => {
                error!("Block encoding failed. Error: {:?}", e);
                return false
            }
        };
        assert!(&vote.node_public_key.verify(&encoded_block[..], &vote.vote));

        let nodes_count = self.participants.len();
        self.zero_participants_check();

        let vote_block = self.find_block(&vote.block);
        if vote_block == None {
            debug!("Received vote for unknown block.");
            if !self.orphan_votes.contains(vote) {
                self.orphan_votes.push(vote.clone());
            }
            return false
        }

        let (unwrapped_vote_block, blockchain_index) = vote_block.unwrap();
        if !unwrapped_vote_block.metadata.sm.votes.contains(vote) {
            unwrapped_vote_block.metadata.sm.votes.push(vote.clone());

            if !unwrapped_vote_block.metadata.sm.notarized &&
                unwrapped_vote_block.metadata.sm.votes.len() > (2 * nodes_count / 3)
            {
                unwrapped_vote_block.metadata.sm.notarized = true;
                self.check_blockchain_finalization(blockchain_index);
            }

            // updating participant vote
            let mut participant = self.participants.get(&vote.id).unwrap().clone();
            participant.voted = Some(vote.block.sl);
            self.participants.insert(participant.id, participant);

            return true
        }
        false
    }

    /// Node searches it the blockchains it holds for provided block.
    pub fn find_block(&mut self, vote_block: &BlockProposal) -> Option<(&mut Block, i64)> {
        for (index, blockchain) in &mut self.node_blockchains.iter_mut().enumerate() {
            for block in blockchain.blocks.iter_mut().rev() {
                if proposal_eq_block(vote_block, block) {
                    return Some((block, index as i64))
                }
            }
        }

        for block in &mut self.canonical_blockchain.blocks.iter_mut().rev() {
            if proposal_eq_block(vote_block, block) {
                return Some((block, -1))
            }
        }
        None
    }

    /// Node checks if the index blockchain can be finalized.
    /// Consensus finalization logic: If node has observed the notarization of 3 consecutive
    /// blocks in a fork chain, it finalizes (appends to canonical blockchain) all blocks up to the middle block.
    /// When fork chain blocks are finalized, rest fork chains not starting by those blocks are removed.
    pub fn check_blockchain_finalization(&mut self, blockchain_index: i64) {
        let blockchain = if blockchain_index == -1 {
            &mut self.canonical_blockchain
        } else {
            &mut self.node_blockchains[blockchain_index as usize]
        };

        let blockchain_len = blockchain.blocks.len();
        if blockchain_len > 2 {
            let mut consecutive_notarized = 0;
            for block in &blockchain.blocks {
                if block.metadata.sm.notarized {
                    consecutive_notarized += 1;
                } else {
                    break
                }
            }

            if consecutive_notarized > 2 {
                let mut finalized_blocks = Vec::new();
                for block in &mut blockchain.blocks[..(consecutive_notarized - 1)] {
                    block.metadata.sm.finalized = true;
                    finalized_blocks.push(block.clone());
                    for tx in block.txs.clone() {
                        if let Some(pos) = self.unconfirmed_txs.iter().position(|txs| *txs == tx) {
                            self.unconfirmed_txs.remove(pos);
                        }
                    }
                }
                blockchain.blocks.drain(0..(consecutive_notarized - 1));
                for block in &finalized_blocks {
                    self.canonical_blockchain.blocks.push(block.clone());
                }

                let mut hasher = DefaultHasher::new();
                let last_finalized_block = self.canonical_blockchain.blocks.last().unwrap();
                last_finalized_block.hash(&mut hasher);
                let last_finalized_block_hash = hasher.finish().to_string();
                let mut dropped_blockchains = Vec::new();
                for (index, blockchain) in self.node_blockchains.iter().enumerate() {
                    let first_block = blockchain.blocks.first().unwrap();
                    if first_block.st != last_finalized_block_hash ||
                        first_block.sl <= last_finalized_block.sl
                    {
                        dropped_blockchains.push(index);
                    }
                }
                for index in dropped_blockchains {
                    self.node_blockchains.remove(index);
                }

                // Remove orphan votes
                let mut orphans = Vec::new();
                for (index, vote) in self.orphan_votes.iter().enumerate() {
                    if vote.block.sl <= last_finalized_block.sl {
                        orphans.push(index);
                    }
                }
                for index in orphans {
                    self.orphan_votes.remove(index);
                }
            }
        }
    }

    /// Node retreives a new participant and appends it to the pending participants list.
    pub fn append_participant(&mut self, participant: Participant) -> bool {
        if self.pending_participants.contains(&participant) {
            return false
        }
        self.pending_participants.push(participant);
        true
    }

    /// This prevent the extreme case scenario where network is initialized, but some nodes
    /// have not pushed the initial participants in the map.
    pub fn zero_participants_check(&mut self) {
        if self.participants.len() == 0 {
            for participant in &self.pending_participants {
                self.participants.insert(participant.id, participant.clone());
            }
            self.pending_participants = Vec::new();
        }
    }

    /// Node refreshes participants map, to retain only the active ones.
    /// Active nodes are considered those who joined or voted on previous epoch.
    pub fn refresh_participants(&mut self) {
        // adding pending participants
        for participant in &self.pending_participants {
            self.participants.insert(participant.id, participant.clone());
        }
        self.pending_participants = Vec::new();

        let mut inactive = Vec::new();
        let previous_epoch = self.get_current_epoch() - 1;
        for (index, participant) in self.participants.clone().iter() {
            match participant.voted {
                Some(epoch) => {
                    if epoch < previous_epoch {
                        inactive.push(index.clone());
                    }
                }
                None => {
                    if participant.joined < previous_epoch {
                        inactive.push(index.clone());
                    }
                }
            }
        }
        for index in inactive {
            self.participants.remove(&index);
        }
    }

    /// Util function to save the current node state to provided file path.
    pub fn save(&self, db: &sled::Db) -> Result<()> {
        let tree = db.open_tree(SLED_STATE_TREE).unwrap();
        let serialized = serialize(self);
        match tree.insert(self.id.to_ne_bytes(), serialized) {
            Err(_) => Err(Error::OperationFailed),
            _ => Ok(()),
        }
    }

    /// Util function to load current node state by the provided file path.
    //  If file is not found, node state is reset.
    pub fn load_or_create(genesis: i64, id: u64, db: &sled::Db) -> Result<State> {
        let tree = db.open_tree(SLED_STATE_TREE).unwrap();
        if let Some(found) = tree.get(id.to_ne_bytes()).unwrap() {
            Ok(deserialize(&found).unwrap())
        } else {
            Self::reset(genesis, id, db)
        }
    }

    /// Util function to load the current node state by the provided folder path.
    pub fn load_current_state(genesis: i64, id: u64, db: &sled::Db) -> Result<StatePtr> {
        let state = Self::load_or_create(genesis, id, db)?;
        Ok(Arc::new(RwLock::new(state)))
    }

    /// Util function to reset node state.
    pub fn reset(genesis: i64, id: u64, db: &sled::Db) -> Result<State> {
        // Genesis block is generated.
        let mut genesis_block = Block::new(
            String::from("⊥"),
            0,
            vec![],
            String::from("proof"),
            String::from("r"),
            String::from("s"),
            vec![],
        );
        genesis_block.metadata.sm.notarized = true;
        genesis_block.metadata.sm.finalized = true;

        let genesis_time = Timestamp(genesis);

        let state = Self::new(id, genesis_time, genesis_block);
        state.save(db)?;
        Ok(state)
    }
}