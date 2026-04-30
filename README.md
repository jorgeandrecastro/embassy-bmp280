# embassy-bmp280

[![Crates.io](https://img.shields.io/crates/v/embassy-bmp280.svg)](https://crates.io/crates/embassy-bmp280)
[![Documentation](https://img.shields.io/docsrs/embassy-bmp280/latest.svg)](https://docs.rs/embassy-bmp280)
[![License](https://img.shields.io/crates/l/embassy-bmp280.svg)](LICENSE)

**Driver asynchrone `no_std` pour le capteur pression/température Bosch BMP280.**

Conçu dans la même philosophie que [`embassy-gy-bmi160`](https://crates.io/crates/embassy-gy-bmi160) :
async natif, zéro allocation, zéro `unsafe`, compatible bus I2C partagé.

---

## ⚡ Caractéristiques

- `#![forbid(unsafe_code)]` :sécurité garantie à la compilation
- Async natif via `embedded-hal-async`
- Reset logiciel + vérification Chip ID au démarrage
- Lecture et application de la **calibration OTP** (algorithme Bosch officiel)
- Compensation **température** et **pression** entièrement entière (`no_std` pur)
- Gestion d'erreurs typée : `Bmp280Error<E>` avec variants distincts
- Compatible `embassy-embedded-hal` (bus I2C partagé multi-périphériques)
- Feature `float` optionnelle pour les conversions `f32`

---

## 📦 Installation

```toml
[dependencies]
embassy-bmp280       = "0.1.0"
embassy-time         = { version = ">=0.3, <0.6" }
embassy-sync         = { version = ">=0.4, <0.9" }
embedded-hal-async   = { version = "1.0" }
```

Pour activer les méthodes de conversion `f32` (`temperature_celsius()`, `pressure_hpa()`) :

```toml
embassy-bmp280 = { version = "0.1.0", features = ["float"] }
```

---

## 🔴 Câblage (Pico 2 / RP2350)

| Broche BMP280 | Connexion         | Note                          |
|---------------|-------------------|-------------------------------|
| VCC           | 3.3V              |                               |
| GND           | Masse commune     |                               |
| SCL           | GP5               | Pull-up recommandé (4.7 kΩ)  |
| SDA           | GP4               | Pull-up recommandé (4.7 kΩ)  |
| SDO           | GND → addr `0x76` | VCC → addr `0x77`             |
| CSB           | 3.3V              | Active le mode I2C            |

---

## 🚀 Utilisation

### Initialisation

```rust
use embassy_bmp280::{Bmp280, Bmp280Address, Bmp280Config, PowerMode};

// Adresse par défaut (SDO au GND)
let mut bmp = Bmp280::new(i2c_device, Bmp280Address::Default, Bmp280Config::default()).await?;
```

`Bmp280::new` effectue dans l'ordre :
1. Reset logiciel du capteur (+ délai datasheet 3 ms)
2. Vérification du Chip ID (0x58 ou 0x60 acceptés)
3. Lecture des 24 octets de calibration OTP
4. Application de la configuration (oversampling + mode)

### Lecture d'une mesure

```rust
let data = bmp.read().await?;

// Température en centidegrés (i32) : 2315 = 23.15 °C
let temp_cdeg = data.temperature_cdeg;

// Pression en Pascals × 256 (format Q24.8)
let press_raw = data.pressure_pa256;
let press_pa  = press_raw / 256;        // Pascals (entier)
let press_hpa = press_raw as f32 / (256.0 * 100.0); // hPa (avec feature "float")
```

### Avec la feature `float`

```rust
// Activez features = ["float"] dans Cargo.toml
let temp_c   = data.temperature_celsius(); // f32, ex: 23.15
let press_hpa = data.pressure_hpa();       // f32, ex: 1013.25
```

### Configuration personnalisée

```rust
use embassy_bmp280::{Bmp280Config, OversamplingTemp, OversamplingPress, PowerMode};

let config = Bmp280Config {
    temp_os:  OversamplingTemp::X4,    // Précision maximale
    press_os: OversamplingPress::X4,
    mode:     PowerMode::Normal,       // Mesures continues
};

let mut bmp = Bmp280::new(i2c_device, Bmp280Address::Default, config).await?;
```

| `OversamplingTemp` / `OversamplingPress` | Résolution | Courant typ. |
|------------------------------------------|------------|--------------|
| `Skip`                                   | désactivé  | —            |
| `X1`                                     | 16 bit     | minimal      |
| `X2`                                     | 17 bit     |              |
| `X4`                                     | 18 bit     |              |
| `X8`                                     | 19 bit     |              |
| `X16`                                    | 20 bit     | maximal      |

### Gestion de l'adresse alternative

```rust
// SA0 / SDO relié au 3.3V
let mut bmp = Bmp280::new(i2c, Bmp280Address::Secondary, Bmp280Config::default()).await?;

// Ou dynamiquement après une redétection
bmp.set_address(Bmp280Address::Custom(0x77));
```

### Gestion du mode veille

```rust
// Économie d'énergie entre les acquisitions
bmp.set_mode(PowerMode::Sleep).await?;

// Reprise sans reconfigurer l'oversampling
bmp.set_mode(PowerMode::Normal).await?;
```

### Gestion d'erreurs

```rust
use embassy_bmp280::Bmp280Error;

match Bmp280::new(i2c, Bmp280Address::Default, Bmp280Config::default()).await {
    Ok(bmp)  => { /* capteur prêt */ }

    Err(Bmp280Error::InvalidChipId(id)) => {
        // Mauvais composant sur le bus — l'ID reçu est disponible pour le debug
        defmt::error!("Chip ID inattendu : 0x{:02X}", id);
    }

    Err(Bmp280Error::InvalidCalibration) => {
        // OTP corrompue ou composant défectueux
        defmt::error!("Calibration invalide");
    }

    Err(Bmp280Error::I2c(e)) => {
        // Erreur de bus (NACK, timeout, arbitration lost…)
        defmt::error!("Erreur I2C : {:?}", e);
    }
}
```

---



## 📡 Signaux globaux

`BMP280_SIGNAL` permet de partager la dernière mesure entre tâches sans
mémoire partagée explicite.

```rust
use embassy_bmp280::signals::BMP280_SIGNAL;

// Tâche productrice : déjà appelée dans system_task ci-dessus
BMP280_SIGNAL.signal(data);

// Tâche consommatrice (ex: logger, radio, autre affichage)
#[embassy_executor::task]
async fn logger_task() {
    loop {
        let data = BMP280_SIGNAL.wait().await;
        // data.temperature_cdeg, data.pressure_pa256
    }
}
```

---

## 🏗️ Architecture du projet

```
embassy-bmp280/
├── Cargo.toml
└── src/
    ├── lib.rs           ← Driver principal, types publics
    ├── error.rs         ← Bmp280Error<E>
    ├── calibration.rs   ← CalibrationData + algorithmes Bosch
    └── signals.rs       ← BMP280_SIGNAL (CriticalSectionRawMutex)
```

### Pourquoi ce découpage ?

`calibration.rs` isole l'arithmétique Bosch — testable indépendamment, sans I2C.
`error.rs` expose un type dédié : le `?` fonctionne partout sans `.map_err` répétitif.
`signals.rs` offre un canal de diffusion inter-tâches sûr depuis les interruptions.

---

## 📋 API publique : référence rapide

### `Bmp280`

| Méthode | Description |
|---------|-------------|
| `Bmp280::new(i2c, addr, config).await` | Crée, initialise et configure le driver |
| `bmp.read().await` | Lit et retourne `Bmp280Data` compensé |
| `bmp.set_mode(mode).await` | Change le mode sans toucher l'oversampling |
| `bmp.set_address(addr)` | Met à jour l'adresse I2C dynamiquement |

### `Bmp280Address`

| Variant | Valeur |
|---------|--------|
| `Default` | `0x76` (SDO au GND) |
| `Secondary` | `0x77` (SDO au VCC) |
| `Custom(u8)` | Valeur libre |

### `Bmp280Data`

| Champ | Type | Valeur |
|-------|------|--------|
| `temperature_cdeg` | `i32` | °C × 100 (ex: `2315` = 23.15 °C) |
| `pressure_pa256` | `u32` | Pa × 256 : format Q24.8 |
| `temperature_celsius()` ¹ | `f32` | °C décimaux |
| `pressure_hpa()` ¹ | `f32` | hPa décimaux |

¹ Disponible uniquement avec `features = ["float"]`.

### `Bmp280Error<E>`

| Variant | Cause |
|---------|-------|
| `I2c(E)` | Erreur de bus (NACK, timeout…) |
| `InvalidChipId(u8)` | ID lu ≠ 0x58 / 0x60 : mauvais composant |
| `InvalidCalibration` | OTP corrompue (dig_T1 == 0) |

---

## 📜 Licence

GPL-2.0-or-later — voir [LICENSE](LICENSE).

Copyright (C) 2026 Jorge Andre Castro

Pour le peuple, les makers : merci Rust, merci Raspberry Pi. 🦅