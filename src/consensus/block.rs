use serde::{Deserialize, Serialize};
use std::{
    hash::{Hash, Hasher},
    io,
};

use super::{metadata::Metadata, participant::Participant, tx::Tx};

use crate::{
    crypto::{keypair::PublicKey, schnorr::Signature},
    impl_vec, net,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

/// This struct represents a tuple of the form (st, sl, txs, metadata).
/// Each blocks parent hash h may be computed simply as a hash of the parent block.
#[derive(Debug, Clone, Deserialize, Serialize, SerialEncodable, SerialDecodable)]
pub struct Block {
    /// Previous block hash
    pub st: String, // Change this to a proper hash type
    /// Slot uid, generated by the beacon
    pub sl: u64,
    /// Transactions payload
    pub txs: Vec<Tx>,
    /// Additional block information
    pub metadata: Metadata,
}

impl Block {
    pub fn new(
        st: String,
        sl: u64,
        txs: Vec<Tx>,
        proof: String,
        r: String,
        s: String,
        participants: Vec<Participant>,
    ) -> Block {
        Block { st, sl, txs, metadata: Metadata::new(proof, r, s, participants) }
    }
}

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        self.st == other.st && self.sl == other.sl && self.txs == other.txs
    }
}

impl Hash for Block {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        format!("{:?}{:?}{:?}", self.st, self.sl, self.txs).hash(hasher);
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, SerialEncodable, SerialDecodable)]
pub struct BlockProposal {
    /// leader public key
    pub public_key: PublicKey,
    /// signed block
    pub signature: Signature,
    /// leader id
    pub id: u64,
    /// Previous block hash
    pub st: String, // Change this to a proper hash type
    /// Slot uid, generated by the beacon
    pub sl: u64,
    /// Transactions payload
    pub txs: Vec<Tx>,
}

impl BlockProposal {
    pub fn new(
        public_key: PublicKey,
        signature: Signature,
        id: u64,
        st: String,
        sl: u64,
        txs: Vec<Tx>,
    ) -> BlockProposal {
        BlockProposal { public_key, signature, id, st, sl, txs }
    }
}

impl net::Message for BlockProposal {
    fn name() -> &'static str {
        "proposal"
    }
}

pub fn proposal_eq_block(proposal: &BlockProposal, block: &Block) -> bool {
    proposal.st == block.st && proposal.sl == block.sl && proposal.txs == block.txs
}

impl_vec!(Block);
