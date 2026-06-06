// Kaleidoscope Cinder — MigrateError Display contract test
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

//! `MigrateError::UnknownItem` Display contract — cinder-local.
//!
//! The operator-facing diagnostic ("cannot migrate unknown item
//! \"<id>\" for tenant <tenant>") is asserted end-to-end by the
//! `kaleidoscope-cli` subprocess tests, but the per-crate mutation
//! gate runs `cargo mutants --package cinder`, which never executes
//! the cli crate's tests. Without a cinder-local assertion the
//! `Display::fmt -> Ok(Default::default())` (empty message) mutant
//! survives. This test closes that cross-crate coverage gap.

use aegis::TenantId;
use cinder::{ItemId, MigrateError};

#[test]
fn unknown_item_display_quotes_bare_item_id_and_names_tenant() {
    let error = MigrateError::UnknownItem {
        tenant: TenantId("acme".to_string()),
        item: ItemId::new("ghost"),
    };

    let rendered = error.to_string();

    // Operator contract: the bare id appears quoted, the tenant is named.
    assert_eq!(
        rendered,
        r#"cannot migrate unknown item "ghost" for tenant acme"#
    );
    // Guards the revert-to-`{item:?}` mutant: the Debug form of the
    // ItemId newtype would leak the `ItemId(...)` wrapper.
    assert!(
        !rendered.contains("ItemId("),
        "Display must not leak the ItemId newtype wrapper: {rendered}"
    );
}
