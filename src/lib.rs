// Copyright (C) 2026 Jorge Andre Castro
// GPL-2.0-or-later

//! # embassy-bmp280
//!
//! Driver asynchrone `no_std` pour le capteur pression/température Bosch BMP280.
//!
//! ## Caractéristiques
//! - Async natif via `embedded-hal-async`
//! - Lecture calibration OTP au démarrage
//! - Compensation température et pression (algorithme Bosch officiel)
//! - Compatible bus I2C partagé (`embassy-embedded-hal`)
//! - zéro `unsafe`
//!
//! ## Exemple minimal
//! ```rust,ignore
//! let mut bmp = Bmp280::new(i2c_device, Bmp280Address::Default).await?;
//! let data = bmp.read().await?;
//! // data.temperature_cdeg : i32 (°C × 100, ex: 2315 = 23.15 °C)
//! // data.pressure_pa256   : u32 (Pa × 256, format Q24.8)
//! ```

#![no_std]
#![forbid(unsafe_code)]

pub mod calibration;
pub mod error;
pub mod signals;

use calibration::CalibrationData;
pub use error::Bmp280Error;

use embassy_time::Timer;
use embedded_hal_async::i2c::I2c;

// Registres 
const REG_CALIB_START:  u8 = 0x88;
const REG_CHIP_ID:      u8 = 0xD0;
const REG_RESET:        u8 = 0xE0;
const REG_CTRL_MEAS:    u8 = 0xF4;
const REG_CONFIG:       u8 = 0xF5;
const REG_PRESS_MSB:    u8 = 0xF7;

const CHIP_ID_BMP280:   u8 = 0x58;
const CHIP_ID_BME280:   u8 = 0x60; // variante compatible
const RESET_WORD:       u8 = 0xB6;

// Adresse I2C

/// Adresse I2C du BMP280.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bmp280Address {
    /// SDO relié au GND → 0x76 (défaut usine).
    Default,
    /// SDO relié au VCC → 0x77.
    Secondary,
    /// Adresse personnalisée.
    Custom(u8),
}

impl From<Bmp280Address> for u8 {
    fn from(a: Bmp280Address) -> u8 {
        match a {
            Bmp280Address::Default    => 0x76,
            Bmp280Address::Secondary  => 0x77,
            Bmp280Address::Custom(v)  => v,
        }
    }
}

// Configuration
/// Oversampling pour la température.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OversamplingTemp {
    Skip  = 0b000,
    X1    = 0b001,
    X2    = 0b010,
    X4    = 0b011,
    X8    = 0b100,
    X16   = 0b101,
}

/// Oversampling pour la pression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OversamplingPress {
    Skip  = 0b000,
    X1    = 0b001,
    X2    = 0b010,
    X4    = 0b011,
    X8    = 0b100,
    X16   = 0b101,
}

/// Mode de fonctionnement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PowerMode {
    /// Pas de mesure. Utile pour économiser l'énergie.
    Sleep  = 0b00,
    /// Une mesure puis retour en sleep.
    Forced = 0b01,
    /// Mesures continues (recommandé avec signaux).
    Normal = 0b11,
}

/// Configuration complète du capteur.
#[derive(Debug, Clone, Copy)]
pub struct Bmp280Config {
    pub temp_os:  OversamplingTemp,
    pub press_os: OversamplingPress,
    pub mode:     PowerMode,
}

impl Default for Bmp280Config {
    /// Équilibre performances/consommation : oversampling ×1, mode Normal.
    fn default() -> Self {
        Self {
            temp_os:  OversamplingTemp::X1,
            press_os: OversamplingPress::X1,
            mode:     PowerMode::Normal,
        }
    }
}

// Données de sortie 

/// Mesure compensée provenant du BMP280.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bmp280Data {
    /// Température en centidegrés Celsius (ex: `2315` = 23.15 °C).
    pub temperature_cdeg: i32,
    /// Pression en Pascals × 256 : format Q24.8.
    /// Diviser par 256 pour obtenir des Pascals.
    pub pressure_pa256: u32,
}

impl Bmp280Data {
    /// Température en degrés Celsius (f32).
    /// 
    /// Note : Nécessite le support matériel FPU de la cible. 
    /// Aucune dépendance externe (libm) n'est requise pour ces divisions simples.
    #[cfg(feature = "float")]
    pub fn temperature_celsius(&self) -> f32 {
        self.temperature_cdeg as f32 / 100.0
    }

    /// Pression en hPa (f32). 1 hPa = 100 Pa.
    #[cfg(feature = "float")]
    pub fn pressure_hpa(&self) -> f32 {
        (self.pressure_pa256 as f32) / (256.0 * 100.0)
    }
}

//  Driver 
pub struct Bmp280<I2C> {
    i2c:   I2C,
    addr:  u8,
    calib: CalibrationData,
}

impl<I2C: I2c> Bmp280<I2C> {
    /// Crée et initialise le driver.
    ///
    /// Effectue dans l'ordre :
    /// 1. Reset logiciel du capteur
    /// 2. Vérification du Chip ID (0x58 ou 0x60)
    /// 3. Lecture des 24 octets de calibration OTP
    /// 4. Application de la configuration (oversampling + mode)
    ///
    /// # Erreurs
    /// - [`Bmp280Error::I2c`] : échec de communication
    /// - [`Bmp280Error::InvalidChipId`] : mauvais composant sur le bus
    /// - [`Bmp280Error::InvalidCalibration`] :OTP corrompue (dig_T1 == 0)
    pub async fn new(
        i2c:    I2C,
        addr:   Bmp280Address,
        config: Bmp280Config,
    ) -> Result<Self, Bmp280Error<I2C::Error>> {
        let mut driver = Self {
            i2c,
            addr: addr.into(),
            calib: CalibrationData::default(),
        };

        driver.soft_reset().await?;
        driver.verify_chip_id().await?;
        driver.read_calibration().await?;
        driver.apply_config(config).await?;

        Ok(driver)
    }

    // Privé 
    async fn write_reg(
        &mut self,
        reg: u8,
        val: u8,
    ) -> Result<(), Bmp280Error<I2C::Error>> {
        self.i2c
            .write(self.addr, &[reg, val])
            .await
            .map_err(Bmp280Error::I2c)
    }

    async fn read_reg(
        &mut self,
        reg: u8,
    ) -> Result<u8, Bmp280Error<I2C::Error>> {
        let mut buf = [0u8; 1];
        self.i2c
            .write_read(self.addr, &[reg], &mut buf)
            .await
            .map_err(Bmp280Error::I2c)?;
        Ok(buf[0])
    }

    async fn read_bytes<const N: usize>(
        &mut self,
        reg: u8,
    ) -> Result<[u8; N], Bmp280Error<I2C::Error>> {
        let mut buf = [0u8; N];
        self.i2c
            .write_read(self.addr, &[reg], &mut buf)
            .await
            .map_err(Bmp280Error::I2c)?;
        Ok(buf)
    }

    async fn soft_reset(&mut self) -> Result<(), Bmp280Error<I2C::Error>> {
        self.write_reg(REG_RESET, RESET_WORD).await?;
        // Le BMP280 a besoin de ~2 ms après un reset pour être prêt à communiquer.
        Timer::after_millis(3).await;
        Ok(())
    }

    async fn verify_chip_id(&mut self) -> Result<(), Bmp280Error<I2C::Error>> {
        let id = self.read_reg(REG_CHIP_ID).await?;
        if id != CHIP_ID_BMP280 && id != CHIP_ID_BME280 {
            return Err(Bmp280Error::InvalidChipId(id));
        }
        Ok(())
    }

    async fn read_calibration(&mut self) -> Result<(), Bmp280Error<I2C::Error>> {
        let raw: [u8; 24] = self.read_bytes::<24>(REG_CALIB_START).await?;
        let calib = CalibrationData::from_raw(&raw);

        // Sanity check : dig_T1 ne peut pas être 0 sur un capteur valide
        if calib.dig_t1 == 0 {
            return Err(Bmp280Error::InvalidCalibration);
        }

        self.calib = calib;
        Ok(())
    }

    async fn apply_config(
        &mut self,
        cfg: Bmp280Config,
    ) -> Result<(), Bmp280Error<I2C::Error>> {
        // ctrl_meas : [7:5] osrs_t | [4:2] osrs_p | [1:0] mode
        let ctrl = ((cfg.temp_os  as u8) << 5)
                 | ((cfg.press_os as u8) << 2)
                 |  (cfg.mode     as u8);
        self.write_reg(REG_CTRL_MEAS, ctrl).await?;

        // config : filtre IIR désactivé, standby 0.5 ms
        self.write_reg(REG_CONFIG, 0x00).await?;
        Ok(())
    }

    // Public

    /// Lit et retourne une mesure compensée (température + pression).
    ///
    /// Lit 6 octets en burst depuis 0xF7, applique l'algorithme de
    /// compensation Bosch et retourne [`Bmp280Data`].
    ///
    /// # Erreurs
    /// - [`Bmp280Error::I2c`] échec de lecture
    pub async fn read(&mut self) -> Result<Bmp280Data, Bmp280Error<I2C::Error>> {
        // Burst read : press[2:0] puis temp[2:0] (ordre datasheet)
        let raw: [u8; 6] = self.read_bytes::<6>(REG_PRESS_MSB).await?;

        let raw_p = ((raw[0] as u32) << 12)
                  | ((raw[1] as u32) <<  4)
                  | ((raw[2] as u32) >>  4);

        let raw_t = ((raw[3] as u32) << 12)
                  | ((raw[4] as u32) <<  4)
                  | ((raw[5] as u32) >>  4);

        let (temperature_cdeg, t_fine) = self.calib.compensate_temperature(raw_t);
        let pressure_pa256 = self.calib
            .compensate_pressure(raw_p, t_fine)
            .unwrap_or(0);

        Ok(Bmp280Data { temperature_cdeg, pressure_pa256 })
    }

    /// Change l'adresse I2C dynamiquement (ex: après une redétection).
    pub fn set_address(&mut self, addr: Bmp280Address) {
        self.addr = addr.into();
    }

    /// Change le mode de fonctionnement sans reconfigurer l'oversampling.
    ///
    /// Utile pour passer en `Sleep` pendant les phases basse consommation.
    pub async fn set_mode(
        &mut self,
        mode: PowerMode,
    ) -> Result<(), Bmp280Error<I2C::Error>> {
        let current = self.read_reg(REG_CTRL_MEAS).await?;
        let updated = (current & 0b1111_1100) | (mode as u8);
        self.write_reg(REG_CTRL_MEAS, updated).await
    }
}