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

use crate::{validation, OpSchema, Schema};

/// Trait used for internal schema validation against some root schema
pub trait SchemaVerify {
    fn schema_verify(&self, root: &Self) -> validation::Status;
}

impl SchemaVerify for Schema {
    fn schema_verify(&self, root: &Schema) -> validation::Status {
        let mut status = validation::Status::new();

        if root.subset_of.is_some() {
            status.add_failure(validation::Failure::SchemaRootHierarchy);
        }

        for (field_type, data_format) in &self.global_types {
            match root.global_types.get(field_type) {
                None => {
                    status.add_failure(validation::Failure::SchemaRootNoFieldTypeMatch(*field_type))
                }
                Some(root_data_format) if root_data_format != data_format => {
                    status.add_failure(validation::Failure::SchemaRootNoFieldTypeMatch(*field_type))
                }
                _ => &status,
            };
        }

        for (assignments_type, state_schema) in &self.owned_types {
            match root.owned_types.get(assignments_type) {
                None => status.add_failure(validation::Failure::SchemaRootNoOwnedRightTypeMatch(
                    *assignments_type,
                )),
                Some(root_state_schema) if root_state_schema != state_schema => status.add_failure(
                    validation::Failure::SchemaRootNoOwnedRightTypeMatch(*assignments_type),
                ),
                _ => &status,
            };
        }

        for valencies_type in &self.valency_types {
            match root.valency_types.contains(valencies_type) {
                false => status.add_failure(validation::Failure::SchemaRootNoPublicRightTypeMatch(
                    *valencies_type,
                )),
                _ => &status,
            };
        }

        status += self.genesis.schema_verify(&root.genesis);

        for (transition_type, transition_schema) in &self.transitions {
            if let Some(root_transition_schema) = root.transitions.get(transition_type) {
                status += transition_schema.schema_verify(root_transition_schema);
            } else {
                status.add_failure(validation::Failure::SchemaRootNoTransitionTypeMatch(
                    *transition_type,
                ));
            }
        }
        for (extension_type, extension_schema) in &self.extensions {
            if let Some(root_extension_schema) = root.extensions.get(extension_type) {
                status += extension_schema.schema_verify(root_extension_schema);
            } else {
                status.add_failure(validation::Failure::SchemaRootNoExtensionTypeMatch(
                    *extension_type,
                ));
            }
        }

        status
    }
}

impl<T> SchemaVerify for T
where T: OpSchema
{
    fn schema_verify(&self, root: &Self) -> validation::Status {
        let mut status = validation::Status::new();
        let node_type = self.op_type();

        for (field_type, occ) in self.global_state() {
            match root.global_state().get(field_type) {
                None => status.add_failure(validation::Failure::SchemaRootNoMetadataMatch(
                    node_type,
                    *field_type,
                )),
                Some(root_occ) if occ != root_occ => status.add_failure(
                    validation::Failure::SchemaRootNoMetadataMatch(node_type, *field_type),
                ),
                _ => &status,
            };
        }

        for (assignments_type, occ) in self.closes() {
            match root.closes().get(assignments_type) {
                None => {
                    status.add_failure(validation::Failure::SchemaRootNoParentOwnedRightsMatch(
                        node_type,
                        *assignments_type,
                    ))
                }
                Some(root_occ) if occ != root_occ => {
                    status.add_failure(validation::Failure::SchemaRootNoParentOwnedRightsMatch(
                        node_type,
                        *assignments_type,
                    ))
                }
                _ => &status,
            };
        }

        for (assignments_type, occ) in self.owned_state() {
            match root.owned_state().get(assignments_type) {
                None => status.add_failure(validation::Failure::SchemaRootNoOwnedRightsMatch(
                    node_type,
                    *assignments_type,
                )),
                Some(root_occ) if occ != root_occ => status.add_failure(
                    validation::Failure::SchemaRootNoOwnedRightsMatch(node_type, *assignments_type),
                ),
                _ => &status,
            };
        }

        for valencies_type in self.redeems() {
            if !root.redeems().contains(valencies_type) {
                status.add_failure(validation::Failure::SchemaRootNoParentPublicRightsMatch(
                    node_type,
                    *valencies_type,
                ));
            }
        }

        for valencies_type in self.valencies() {
            if !root.valencies().contains(valencies_type) {
                status.add_failure(validation::Failure::SchemaRootNoPublicRightsMatch(
                    node_type,
                    *valencies_type,
                ));
            }
        }

        status
    }
}
