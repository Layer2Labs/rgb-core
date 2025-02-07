// RGB Core Library: consensus layer for RGB smart contracts.
//
// SPDX-License-Identifier: Apache-2.0
//
// Written in 2019-2023 by
//     Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2019-2023 LNP/BP Standards Association. All rights reserved.
// Copyright (C) 2019-2023 Dr Maxim Orlovsky. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use core::cmp::Ordering;
use core::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::{io, vec};

use amplify::confinement::{Confined, SmallVec, TinyOrdMap, U8};
use amplify::Wrapper;
use commit_verify::merkle::{MerkleLeaves, MerkleNode};
use commit_verify::{CommitEncode, CommitStrategy, CommitmentId, Conceal};
use strict_encoding::{StrictDumb, StrictEncode, StrictWriter};

use super::{attachment, data, fungible, seal, ConfidentialState, RevealedState};
use crate::{schema, LIB_NAME_RGB};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Display, Error)]
#[display(doc_comments)]
/// the requested data are not present.
pub struct UnknownDataError;

#[derive(Wrapper, Clone, PartialEq, Eq, Hash, Debug, From)]
#[wrapper(Deref)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_RGB)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate", transparent)
)]
pub struct GlobalValues(Confined<Vec<data::Revealed>, 1, U8>);

impl StrictDumb for GlobalValues {
    fn strict_dumb() -> Self { Self(confined_vec!(data::Revealed::strict_dumb())) }
}

#[derive(Wrapper, Clone, PartialEq, Eq, Hash, Debug, Default, From)]
#[wrapper(Deref)]
#[derive(StrictType, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_RGB)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate", transparent)
)]
pub struct GlobalState(TinyOrdMap<schema::GlobalStateType, GlobalValues>);

/// Categories of the state
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
pub enum StateType {
    /// No state data
    Void,

    /// Value-based state, i.e. which can be committed to with a Pedersen
    /// commitment
    Fungible,

    /// State defined with custom data
    Structured,

    /// Attached data container
    Attachment,
}

pub trait StatePair: Debug {
    type Confidential: ConfidentialState;
    type Revealed: RevealedState;
}

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Right;
impl StatePair for Right {
    type Confidential = data::VoidState;
    type Revealed = data::VoidState;
}

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Fungible;
impl StatePair for Fungible {
    type Confidential = fungible::Confidential;
    type Revealed = fungible::Revealed;
}

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct State;
impl StatePair for State {
    type Confidential = data::Confidential;
    type Revealed = data::Revealed;
}

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Attach;
impl StatePair for Attach {
    type Confidential = attachment::Confidential;
    type Revealed = attachment::Revealed;
}

/// State data are assigned to a seal definition, which means that they are
/// owned by a person controlling spending of the seal UTXO, unless the seal
/// is closed, indicating that a transfer of ownership had taken place
#[derive(Clone, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(
    lib = LIB_NAME_RGB,
    tags = custom,
    dumb = { Self::Confidential { seal: strict_dumb!(), state: strict_dumb!() } }
)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate", rename_all = "camelCase")
)]
pub enum Assign<Pair>
where
    Pair: StatePair,
    // Deterministic ordering requires Eq operation, so the confidential
    // state must have it
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
    #[strict_type(tag = 0x00)]
    Confidential {
        seal: seal::Confidential,
        state: Pair::Confidential,
    },
    #[strict_type(tag = 0x03)]
    Revealed {
        seal: seal::Revealed,
        state: Pair::Revealed,
    },
    #[strict_type(tag = 0x02)]
    ConfidentialSeal {
        seal: seal::Confidential,
        state: Pair::Revealed,
    },
    #[strict_type(tag = 0x01)]
    ConfidentialState {
        seal: seal::Revealed,
        state: Pair::Confidential,
    },
}

// Consensus-critical!
// Assignment indexes are part of the transition ancestor's commitment, so
// here we use deterministic ordering based on hash values of the concealed
// seal data contained within the assignment
impl<Pair> PartialOrd for Assign<Pair>
where
    Pair: StatePair,
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.to_confidential_seal()
            .partial_cmp(&other.to_confidential_seal())
    }
}

impl<Pair> Ord for Assign<Pair>
where
    Pair: StatePair,
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.to_confidential_seal()
            .cmp(&other.to_confidential_seal())
    }
}

impl<Pair> PartialEq for Assign<Pair>
where
    Pair: StatePair,
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
    fn eq(&self, other: &Self) -> bool {
        self.to_confidential_seal() == other.to_confidential_seal() &&
            self.to_confidential_state() == other.to_confidential_state()
    }
}

impl<Pair> Eq for Assign<Pair>
where
    Pair: StatePair,
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
}

impl<Pair> Hash for Assign<Pair>
where
    Pair: StatePair,
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_confidential_seal().hash(state);
        self.to_confidential_state().hash(state);
    }
}

impl<Pair> Assign<Pair>
where
    Pair: StatePair,
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
    pub fn revealed(seal: seal::Revealed, state: Pair::Revealed) -> Self {
        Assign::Revealed { seal, state }
    }

    pub fn with_seal_replaced(assignment: &Self, seal: seal::Revealed) -> Self {
        match assignment {
            Assign::Confidential { seal: _, state } |
            Assign::ConfidentialState { seal: _, state } => Assign::ConfidentialState {
                seal,
                state: state.clone(),
            },
            Assign::ConfidentialSeal { seal: _, state } | Assign::Revealed { seal: _, state } => {
                Assign::Revealed {
                    seal,
                    state: state.clone(),
                }
            }
        }
    }

    pub fn to_confidential_seal(&self) -> seal::Confidential {
        match self {
            Assign::Revealed { seal, .. } | Assign::ConfidentialState { seal, .. } => {
                seal.conceal()
            }
            Assign::Confidential { seal, .. } | Assign::ConfidentialSeal { seal, .. } => *seal,
        }
    }

    pub fn revealed_seal(&self) -> Option<seal::Revealed> {
        match self {
            Assign::Revealed { seal, .. } | Assign::ConfidentialState { seal, .. } => Some(*seal),
            Assign::Confidential { .. } | Assign::ConfidentialSeal { .. } => None,
        }
    }

    pub fn to_confidential_state(&self) -> Pair::Confidential {
        match self {
            Assign::Revealed { state, .. } | Assign::ConfidentialSeal { state, .. } => {
                state.conceal().into()
            }
            Assign::Confidential { state, .. } | Assign::ConfidentialState { state, .. } => {
                state.clone()
            }
        }
    }

    pub fn as_revealed_state(&self) -> Option<&Pair::Revealed> {
        match self {
            Assign::Revealed { state, .. } | Assign::ConfidentialSeal { state, .. } => Some(state),
            Assign::Confidential { .. } | Assign::ConfidentialState { .. } => None,
        }
    }

    pub fn as_revealed(&self) -> Option<(&seal::Revealed, &Pair::Revealed)> {
        match self {
            Assign::Revealed { seal, state } => Some((seal, state)),
            _ => None,
        }
    }

    pub fn to_revealed(&self) -> Option<(seal::Revealed, Pair::Revealed)> {
        match self {
            Assign::Revealed { seal, state } => Some((*seal, state.clone())),
            _ => None,
        }
    }

    pub fn into_revealed(self) -> Option<(seal::Revealed, Pair::Revealed)> {
        match self {
            Assign::Revealed { seal, state } => Some((seal, state)),
            _ => None,
        }
    }
}

impl<Pair> Conceal for Assign<Pair>
where
    Self: Clone,
    Pair: StatePair,
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
    type Concealed = Self;

    fn conceal(&self) -> Self::Concealed {
        match self {
            Assign::Confidential { .. } => self.clone(),
            Assign::ConfidentialState { seal, state } => Self::Confidential {
                seal: seal.conceal(),
                state: state.clone(),
            },
            Assign::Revealed { seal, state } => Self::Confidential {
                seal: seal.conceal(),
                state: state.conceal().into(),
            },
            Assign::ConfidentialSeal { seal, state } => Self::Confidential {
                seal: *seal,
                state: state.conceal().into(),
            },
        }
    }
}

// We can't use `UsingConceal` strategy here since it relies on the
// `commit_encode` of the concealed type, and here the concealed type is again
// `OwnedState`, leading to a recurrency. So we use `strict_encode` of the
// concealed data.
impl<Pair> CommitEncode for Assign<Pair>
where
    Self: Clone,
    Pair: StatePair,
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
    fn commit_encode(&self, e: &mut impl io::Write) {
        let w = StrictWriter::with(u32::MAX as usize, e);
        self.conceal().strict_encode(w).ok();
    }
}

impl<Pair> CommitmentId for Assign<Pair>
where
    Self: Clone,
    Pair: StatePair,
    Pair::Confidential: PartialEq + Eq,
    Pair::Confidential: From<<Pair::Revealed as Conceal>::Concealed>,
{
    const TAG: [u8; 32] = *b"urn:lnpbp:rgb:owned-state:v1#23A";
    type Id = MerkleNode;
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_RGB, tags = custom, dumb = Self::Declarative(strict_dumb!()))]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate", rename_all = "camelCase")
)]
pub enum TypedState {
    // TODO: Consider using non-empty variants
    #[strict_type(tag = 0x00)]
    Declarative(SmallVec<Assign<Right>>),
    #[strict_type(tag = 0x01)]
    Fungible(SmallVec<Assign<Fungible>>),
    #[strict_type(tag = 0x02)]
    Structured(SmallVec<Assign<State>>),
    #[strict_type(tag = 0xFF)]
    Attachment(SmallVec<Assign<Attach>>),
}

impl TypedState {
    pub fn is_empty(&self) -> bool {
        match self {
            TypedState::Declarative(set) => set.is_empty(),
            TypedState::Fungible(set) => set.is_empty(),
            TypedState::Structured(set) => set.is_empty(),
            TypedState::Attachment(set) => set.is_empty(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            TypedState::Declarative(set) => set.len(),
            TypedState::Fungible(set) => set.len(),
            TypedState::Structured(set) => set.len(),
            TypedState::Attachment(set) => set.len(),
        }
    }

    #[inline]
    pub fn state_type(&self) -> StateType {
        match self {
            TypedState::Declarative(_) => StateType::Void,
            TypedState::Fungible(_) => StateType::Fungible,
            TypedState::Structured(_) => StateType::Structured,
            TypedState::Attachment(_) => StateType::Attachment,
        }
    }

    #[inline]
    pub fn is_declarative(&self) -> bool { matches!(self, TypedState::Declarative(_)) }

    #[inline]
    pub fn is_fungible(&self) -> bool { matches!(self, TypedState::Fungible(_)) }

    #[inline]
    pub fn is_structured(&self) -> bool { matches!(self, TypedState::Structured(_)) }

    #[inline]
    pub fn is_attachment(&self) -> bool { matches!(self, TypedState::Attachment(_)) }

    #[inline]
    pub fn as_declarative(&self) -> &[Assign<Right>] {
        match self {
            TypedState::Declarative(set) => set,
            _ => Default::default(),
        }
    }

    #[inline]
    pub fn as_fungible(&self) -> &[Assign<Fungible>] {
        match self {
            TypedState::Fungible(set) => set,
            _ => Default::default(),
        }
    }

    #[inline]
    pub fn as_structured(&self) -> &[Assign<State>] {
        match self {
            TypedState::Structured(set) => set,
            _ => Default::default(),
        }
    }

    #[inline]
    pub fn as_attachment(&self) -> &[Assign<Attach>] {
        match self {
            TypedState::Attachment(set) => set,
            _ => Default::default(),
        }
    }

    #[inline]
    pub fn as_declarative_mut(&mut self) -> Option<&mut SmallVec<Assign<Right>>> {
        match self {
            TypedState::Declarative(set) => Some(set),
            _ => None,
        }
    }

    #[inline]
    pub fn as_fungible_mut(&mut self) -> Option<&mut SmallVec<Assign<Fungible>>> {
        match self {
            TypedState::Fungible(set) => Some(set),
            _ => None,
        }
    }

    #[inline]
    pub fn as_structured_mut(&mut self) -> Option<&mut SmallVec<Assign<State>>> {
        match self {
            TypedState::Structured(set) => Some(set),
            _ => None,
        }
    }

    #[inline]
    pub fn as_attachment_mut(&mut self) -> Option<&mut SmallVec<Assign<Attach>>> {
        match self {
            TypedState::Attachment(set) => Some(set),
            _ => None,
        }
    }

    /// If seal definition does not exist, returns [`UnknownDataError`]. If the
    /// seal is confidential, returns `Ok(None)`; otherwise returns revealed
    /// seal data packed as `Ok(Some(`[`seal::Revealed`]`))`
    pub fn revealed_seal_at(&self, index: u16) -> Result<Option<seal::Revealed>, UnknownDataError> {
        Ok(match self {
            TypedState::Declarative(vec) => vec
                .get(index as usize)
                .ok_or(UnknownDataError)?
                .revealed_seal(),
            TypedState::Fungible(vec) => vec
                .get(index as usize)
                .ok_or(UnknownDataError)?
                .revealed_seal(),
            TypedState::Structured(vec) => vec
                .get(index as usize)
                .ok_or(UnknownDataError)?
                .revealed_seal(),
            TypedState::Attachment(vec) => vec
                .get(index as usize)
                .ok_or(UnknownDataError)?
                .revealed_seal(),
        })
    }

    pub fn to_confidential_seals(&self) -> Vec<seal::Confidential> {
        match self {
            TypedState::Declarative(s) => s.iter().map(Assign::<_>::to_confidential_seal).collect(),
            TypedState::Fungible(s) => s.iter().map(Assign::<_>::to_confidential_seal).collect(),
            TypedState::Structured(s) => s.iter().map(Assign::<_>::to_confidential_seal).collect(),
            TypedState::Attachment(s) => s.iter().map(Assign::<_>::to_confidential_seal).collect(),
        }
    }

    pub fn as_structured_state_at(
        &self,
        index: u16,
    ) -> Result<Option<&data::Revealed>, UnknownDataError> {
        match self {
            TypedState::Structured(vec) => Ok(vec
                .get(index as usize)
                .ok_or(UnknownDataError)?
                .as_revealed_state()),
            _ => Err(UnknownDataError),
        }
    }

    pub fn as_fungible_state_at(
        &self,
        index: u16,
    ) -> Result<Option<&fungible::Revealed>, UnknownDataError> {
        match self {
            TypedState::Fungible(vec) => Ok(vec
                .get(index as usize)
                .ok_or(UnknownDataError)?
                .as_revealed_state()),
            _ => Err(UnknownDataError),
        }
    }
}

impl CommitStrategy for TypedState {
    type Strategy =
        commit_verify::strategies::Merklize<{ u128::from_be_bytes(*b"rgb:state:owned*") }>;
}

impl MerkleLeaves for TypedState {
    type Leaf = MerkleNode;
    type LeafIter = vec::IntoIter<MerkleNode>;

    fn merkle_leaves(&self) -> Self::LeafIter {
        match self {
            TypedState::Declarative(vec) => vec
                .iter()
                .map(Assign::<Right>::commitment_id)
                .collect::<Vec<_>>(),
            TypedState::Fungible(vec) => vec
                .iter()
                .map(Assign::<Fungible>::commitment_id)
                .collect::<Vec<_>>(),
            TypedState::Structured(vec) => vec
                .iter()
                .map(Assign::<State>::commitment_id)
                .collect::<Vec<_>>(),
            TypedState::Attachment(vec) => vec
                .iter()
                .map(Assign::<Attach>::commitment_id)
                .collect::<Vec<_>>(),
        }
        .into_iter()
    }
}
