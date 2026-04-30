// Copyright (C) 2026 Jorge Andre Castro
// GPL-2.0-or-later

//! Types d'erreur du driver BMP280.

/// Erreurs possibles du driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bmp280Error<E> {
    /// Erreur de communication I2C sous-jacente.
    I2c(E),
    /// Le chip ID lu ne correspond pas au BMP280 (0x58 ou 0x60).
    /// Contient l'ID inattendu reçu.
    InvalidChipId(u8),
    /// Les données de calibration lues sont invalides (ex: dig_T1 == 0).
    InvalidCalibration,
}

impl<E> From<E> for Bmp280Error<E> {
    fn from(e: E) -> Self {
        Self::I2c(e)
    }
}