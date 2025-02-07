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

use core::any::Any;

use amplify::AsAny;
use commit_verify::Conceal;

use crate::contract::assignment::{Attach, Fungible, Right, State};
use crate::schema::OwnedStateType;
use crate::{validation, Assign, OpId, StatePair, StateSchema};

impl StateSchema {
    pub fn validate<STATE>(
        &self,
        // type_system: &TypeSystem,
        node_id: &OpId,
        assignment_id: OwnedStateType,
        data: &Assign<STATE>,
    ) -> validation::Status
    where
        STATE: StatePair,
        STATE::Confidential: PartialEq + Eq,
        STATE::Confidential: From<<STATE::Revealed as Conceal>::Concealed>,
    {
        let mut status = validation::Status::new();
        match data {
            Assign::Confidential { state, .. } | Assign::ConfidentialState { state, .. } => {
                let a: &dyn Any = state.as_any();
                match self {
                    StateSchema::Declarative => {
                        if a.downcast_ref::<<Right as StatePair>::Confidential>()
                            .is_none()
                        {
                            status.add_failure(validation::Failure::SchemaMismatchedStateType(
                                assignment_id,
                            ));
                        }
                    }
                    StateSchema::Fungible(_) => {
                        if let Some(value) =
                            a.downcast_ref::<<Fungible as StatePair>::Confidential>()
                        {
                            // [SECURITY-CRITICAL]: Bulletproofs validation
                            if let Err(err) = value.verify_range_proof() {
                                status.add_failure(validation::Failure::InvalidBulletproofs(
                                    *node_id,
                                    assignment_id,
                                    err.to_string(),
                                ));
                            }
                        } else {
                            status.add_failure(validation::Failure::SchemaMismatchedStateType(
                                assignment_id,
                            ));
                        }

                        // TODO: When other homomorphic formats will be added,
                        //       add information to the status like with hashed
                        //       data below
                    }
                    StateSchema::Structured(_) => match a
                        .downcast_ref::<<State as StatePair>::Confidential>()
                    {
                        None => {
                            status.add_failure(validation::Failure::SchemaMismatchedStateType(
                                assignment_id,
                            ));
                        }
                        Some(_) => {
                            status.add_info(validation::Info::UncheckableConfidentialStateData(
                                *node_id,
                                assignment_id,
                            ));
                        }
                    },
                    StateSchema::Attachment => {
                        if a.downcast_ref::<<Attach as StatePair>::Confidential>()
                            .is_none()
                        {
                            status.add_failure(validation::Failure::SchemaMismatchedStateType(
                                assignment_id,
                            ));
                        }
                    }
                }
            }
            Assign::Revealed { state, .. } | Assign::ConfidentialSeal { state, .. } => {
                let a: &dyn Any = state.as_any();
                match self {
                    StateSchema::Declarative => {
                        if a.downcast_ref::<<Right as StatePair>::Revealed>().is_none() {
                            status.add_failure(validation::Failure::SchemaMismatchedStateType(
                                assignment_id,
                            ));
                        }
                    }
                    StateSchema::Fungible(_format) => {
                        if a.downcast_ref::<<Fungible as StatePair>::Revealed>()
                            .is_none()
                        {
                            status.add_failure(validation::Failure::SchemaMismatchedStateType(
                                assignment_id,
                            ));
                        }
                        // TODO #15: When other homomorphic formats will be
                        // added,       add type check
                        // like with hashed data below
                    }
                    StateSchema::Structured(_semid) => {
                        match a.downcast_ref::<<State as StatePair>::Revealed>() {
                            None => {
                                status.add_failure(validation::Failure::SchemaMismatchedStateType(
                                    assignment_id,
                                ));
                            }
                            Some(_data) => {
                                // TODO: #137 run strict type validation
                            }
                        }
                    }
                    StateSchema::Attachment => {
                        if a.downcast_ref::<<Attach as StatePair>::Revealed>()
                            .is_none()
                        {
                            status.add_failure(validation::Failure::SchemaMismatchedStateType(
                                assignment_id,
                            ));
                        }
                    }
                }
            }
        }
        status
    }
}
