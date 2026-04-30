// Copyright (C) 2026 Jorge Andre Castro
// GPL-2.0-or-later

//! Signaux globaux asynchrones pour le partage de données BMP280.
//!
//! Utilisent `CriticalSectionRawMutex` : sûrs depuis les interruptions.

#![forbid(unsafe_code)]

use crate::Bmp280Data;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

/// Dernière mesure publiée (température + pression compensées).
pub static BMP280_SIGNAL: Signal<CriticalSectionRawMutex, Bmp280Data> = Signal::new();