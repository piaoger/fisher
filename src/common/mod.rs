// Copyright (C) 2017 Pietro Albini
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#![warn(missing_docs)]

//! This module contains all the common code used by the Fisher application.
//! All the other modules then depends on this crate to get access to
//! the features.

pub mod config;
pub mod errors;
pub mod prelude;
pub mod serial;
pub mod state;
pub mod structs;
pub mod traits;
