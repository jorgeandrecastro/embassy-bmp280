// Copyright (C) 2026 Jorge Andre Castro
// GPL-2.0-or-later

//! Données de calibration usine du BMP280.
//!
//! Chaque capteur BMP280 stocke en OTP des coefficients uniques.
//! La compensation est OBLIGATOIRE pour obtenir des valeurs physiques.

/// Coefficients de calibration température/pression.
#[derive(Debug, Clone, Copy, Default)]
pub struct CalibrationData {
    pub dig_t1: u16,
    pub dig_t2: i16,
    pub dig_t3: i16,
    pub dig_p1: u16,
    pub dig_p2: i16,
    pub dig_p3: i16,
    pub dig_p4: i16,
    pub dig_p5: i16,
    pub dig_p6: i16,
    pub dig_p7: i16,
    pub dig_p8: i16,
    pub dig_p9: i16,
}

impl CalibrationData {
    /// Parse les 24 octets bruts depuis les registres 0x88..0x9F.
    pub fn from_raw(raw: &[u8; 24]) -> Self {
        Self {
            dig_t1: u16::from_le_bytes([raw[0],  raw[1]]),
            dig_t2: i16::from_le_bytes([raw[2],  raw[3]]),
            dig_t3: i16::from_le_bytes([raw[4],  raw[5]]),
            dig_p1: u16::from_le_bytes([raw[6],  raw[7]]),
            dig_p2: i16::from_le_bytes([raw[8],  raw[9]]),
            dig_p3: i16::from_le_bytes([raw[10], raw[11]]),
            dig_p4: i16::from_le_bytes([raw[12], raw[13]]),
            dig_p5: i16::from_le_bytes([raw[14], raw[15]]),
            dig_p6: i16::from_le_bytes([raw[16], raw[17]]),
            dig_p7: i16::from_le_bytes([raw[18], raw[19]]),
            dig_p8: i16::from_le_bytes([raw[20], raw[21]]),
            dig_p9: i16::from_le_bytes([raw[22], raw[23]]),
        }
    }

    /// Compensation température : algorithme Bosch officiel.
    ///
    /// Retourne `(température en °C × 100 en i32, t_fine)`.
    /// `t_fine` est réutilisé pour la compensation pression.
    pub fn compensate_temperature(&self, raw_t: u32) -> (i32, i32) {
        let t1 = self.dig_t1 as i32;
        let t2 = self.dig_t2 as i32;
        let t3 = self.dig_t3 as i32;
        let adc = raw_t as i32;

        let var1 = ((adc >> 3) - (t1 << 1)) * t2 >> 11;
        let tmp  = (adc >> 4) - t1;
        let var2 = (((tmp * tmp) >> 12) * t3) >> 14;

        let t_fine = var1 + var2;
        let temp   = (t_fine * 5 + 128) >> 8; // en centidegrés
        (temp, t_fine)
    }

    /// Compensation pression : algorithme Bosch officiel.
    ///
    /// Retourne la pression en Pascals × 256 (format Q24.8).
    /// Retourne `None` si la division par zéro est détectée.
    pub fn compensate_pressure(&self, raw_p: u32, t_fine: i32) -> Option<u32> {
        let adc = raw_p as i64;

        let mut var1 = (t_fine as i64) - 128_000;
        let mut var2 = var1 * var1 * self.dig_p6 as i64;
        var2 += (var1 * self.dig_p5 as i64) << 17;
        var2 += (self.dig_p4 as i64) << 35;
        var1  = ((var1 * var1 * self.dig_p3 as i64) >> 8)
              + ((var1 * self.dig_p2 as i64) << 12);
        var1  = (((1_i64 << 47) + var1) * self.dig_p1 as i64) >> 33;

        if var1 == 0 {
            return None; // division par zéro
        }

        let mut p = 1_048_576_i64 - adc;
        p = (((p << 31) - var2) * 3125) / var1;

        var1 = (self.dig_p9 as i64 * (p >> 13) * (p >> 13)) >> 25;
        var2 = (self.dig_p8 as i64 * p) >> 19;
        p    = ((p + var1 + var2) >> 8) + ((self.dig_p7 as i64) << 4);

        Some(p as u32)
    }
}