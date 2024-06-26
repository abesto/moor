// Copyright (C) 2024 Ryan Daum <ryan.daum@gmail.com>
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, version 3.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with
// this program. If not, see <https://www.gnu.org/licenses/>.
//

use strum::{AsRefStr, Display, EnumCount, EnumIter, EnumProperty};

/// The set of binary relations that are used to represent the world state in the moor system.
#[repr(usize)]
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, EnumIter, EnumCount, Display, EnumProperty, AsRefStr,
)]
pub enum WorldStateTable {
    /// Object<->Parent
    #[strum(props(
        DomainType = "Integer",
        CodomainType = "Integer",
        SecondaryIndexed = "true"
    ))]
    ObjectParent = 0,
    /// Object<->Location
    #[strum(props(
        DomainType = "Integer",
        CodomainType = "Integer",
        SecondaryIndexed = "true"
    ))]
    ObjectLocation = 1,
    /// Object->Flags (BitEnum<ObjFlag>)
    #[strum(props(DomainType = "Integer", CodomainType = "Bytes"))]
    ObjectFlags = 2,
    /// Object->Name
    #[strum(props(DomainType = "Integer", CodomainType = "String"))]
    ObjectName = 3,
    /// Object->Owner
    #[strum(props(DomainType = "Integer", CodomainType = "Integer"))]
    ObjectOwner = 4,
    /// Object->Verbs (Verbdefs)
    #[strum(props(DomainType = "Integer", CodomainType = "Bytes"))]
    ObjectVerbs = 5,
    /// (Object, UUID)->VerbProgram (Binary)
    #[strum(props(
        DomainType = "Bytes",
        CodomainType = "Bytes",
        CompositeDomain = "true",
        Domain_A_Size = "8",
        Domain_B_Size = "16"
    ))]
    VerbProgram = 6,
    /// Object->Properties (Propdefs)
    #[strum(props(DomainType = "Integer", CodomainType = "Bytes"))]
    ObjectPropDefs = 7,
    /// (Object, UUID)->PropertyValue (Var)
    #[strum(props(
        DomainType = "Bytes",
        CodomainType = "Bytes",
        CompositeDomain = "true",
        Domain_A_Size = "8",
        Domain_B_Size = "16"
    ))]
    ObjectPropertyValue = 8,
    /// Object->PropertyPermissions (PropPerms)
    #[strum(props(
        DomainType = "Bytes",
        CodomainType = "Bytes",
        CompositeDomain = "true",
        Domain_A_Size = "8",
        Domain_B_Size = "16"
    ))]
    ObjectPropertyPermissions = 9,
    /// Set of sequences sequence_id -> current_value
    #[strum(props(DomainType = "Bytes", CodomainType = "Bytes"))]
    Sequences = 10,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, EnumIter, EnumCount)]
pub enum WorldStateSequence {
    MaximumObject = 0,
}

impl From<WorldStateSequence> for u8 {
    fn from(val: WorldStateSequence) -> Self {
        val as u8
    }
}

impl From<WorldStateTable> for usize {
    fn from(val: WorldStateTable) -> Self {
        val as usize
    }
}
