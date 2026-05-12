// Kaleidoscope Aegis — tenancy + auth + audit
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

//! Tenant catalogue: TOML-backed at v0.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::validator::TenantId;

/// One tenant in the catalogue. Display name + notes are
/// operator-facing; the platform routes only off `id`.
#[derive(Debug, Clone)]
pub struct TenantRecord {
    pub id: TenantId,
    pub display_name: Option<String>,
    pub notes: Option<String>,
}

/// Typed catalogue. O(1) `contains` via internal `HashSet`.
#[derive(Debug, Clone, Default)]
pub struct TenantCatalogue {
    by_id: HashMap<TenantId, TenantRecord>,
}

impl TenantCatalogue {
    /// Construct from a list of records. Returns `Err` on duplicate id.
    pub fn from_records(records: Vec<TenantRecord>) -> Result<Self, CatalogueError> {
        let mut by_id: HashMap<TenantId, TenantRecord> = HashMap::with_capacity(records.len());
        let mut seen: HashSet<TenantId> = HashSet::with_capacity(records.len());
        for record in records {
            if !seen.insert(record.id.clone()) {
                return Err(CatalogueError::DuplicateTenant(record.id));
            }
            by_id.insert(record.id.clone(), record);
        }
        Ok(Self { by_id })
    }

    /// Is this tenant registered?
    pub fn contains(&self, id: &TenantId) -> bool {
        self.by_id.contains_key(id)
    }

    /// Look up a tenant record. Returns `None` if absent.
    pub fn get(&self, id: &TenantId) -> Option<&TenantRecord> {
        self.by_id.get(id)
    }

    /// Number of registered tenants.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Is the catalogue empty?
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// Catalogue loader / construction errors.
#[derive(Debug)]
pub enum CatalogueError {
    Read { path: PathBuf, message: String },
    Parse { path: PathBuf, message: String },
    DuplicateTenant(TenantId),
}

impl fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogueError::Read { path, message } => {
                write!(
                    f,
                    "cannot read tenant catalogue {}: {message}",
                    path.display()
                )
            }
            CatalogueError::Parse { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            CatalogueError::DuplicateTenant(id) => {
                write!(f, "duplicate tenant id: {id}")
            }
        }
    }
}

impl std::error::Error for CatalogueError {}

/// Load a tenant catalogue from a TOML file. The file is expected
/// to have `[[tenants]]` tables with `id`, optional `display_name`,
/// optional `notes`. Unknown fields are rejected.
pub fn load_catalogue(path: &Path) -> Result<TenantCatalogue, CatalogueError> {
    let text = fs::read_to_string(path).map_err(|err| CatalogueError::Read {
        path: path.to_path_buf(),
        message: err.to_string(),
    })?;
    let parsed: FileShape = toml::from_str(&text).map_err(|err| CatalogueError::Parse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })?;
    let records: Vec<TenantRecord> = parsed
        .tenants
        .into_iter()
        .map(|t| TenantRecord {
            id: TenantId(t.id),
            display_name: t.display_name,
            notes: t.notes,
        })
        .collect();
    TenantCatalogue::from_records(records)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileShape {
    #[serde(default)]
    tenants: Vec<RawTenant>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTenant {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}
