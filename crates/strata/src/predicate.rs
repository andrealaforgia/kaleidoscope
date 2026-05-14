// Kaleidoscope Strata — query predicate
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

//! Query predicate. v0 supports profile_type equality.
//! Sample / location / function predicates land at v1 with the
//! columnar substrate (they are expensive on a linear scan).

use crate::profile::Profile;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Predicate {
    profile_type: Option<String>,
}

impl Predicate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter to profiles whose `profile_type == name` (e.g.
    /// `"cpu"`, `"heap"`, `"goroutine"`).
    pub fn profile_type(mut self, name: impl Into<String>) -> Self {
        self.profile_type = Some(name.into());
        self
    }

    pub fn matches(&self, profile: &Profile) -> bool {
        if let Some(target) = self.profile_type.as_deref() {
            if profile.profile_type != target {
                return false;
            }
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        self.profile_type.is_none()
    }
}
