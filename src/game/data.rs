//! Balance tables for guns, zombies and base structures.
use crate::audio::Sfx;
use crate::render::mesh as m;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fire {
    Hitscan { pellets: u32, spread: f32, pierce: u32 },
    Explosive { speed: f32, radius: f32, arc: bool },
    Flame,
    Cryo,
    Tesla { chains: u32 },
    Beam,
    Saw { bounces: u32 },
}

pub struct WeaponDef {
    pub name: &'static str,
    pub blurb: &'static str,
    pub fire: Fire,
    pub damage: f32,
    pub rate: f32,
    pub mag: u32,
    pub reserve: u32,
    pub reload: f32,
    pub price: u32,
    pub ammo_price: u32,
    pub unlock_wave: u32,
    pub kick: f32,
    pub recoil: f32,
    pub knock: f32,
    /// 0..1: how readily kills from this gun dismember.
    pub gore: f32,
    pub spinup: f32,
    pub sfx: Sfx,
    pub mesh: usize,
    pub muzzle: f32,
    pub tracer: [f32; 3],
    pub two_handed: bool,
}

const TRACER: [f32; 3] = [6.0, 4.5, 2.0];

#[allow(clippy::too_many_arguments)]
const fn hitscan(
    name: &'static str, blurb: &'static str, pellets: u32, spread: f32, pierce: u32, damage: f32, rate: f32,
    mag: u32, reserve: u32, reload: f32, price: u32, ammo_price: u32, unlock_wave: u32, kick: f32,
    recoil: f32, knock: f32, gore: f32, sfx: Sfx, mesh: usize, muzzle: f32, two_handed: bool,
) -> WeaponDef {
    WeaponDef {
        name, blurb, fire: Fire::Hitscan { pellets, spread, pierce }, damage, rate, mag, reserve, reload, price,
        ammo_price, unlock_wave, kick, recoil, knock, gore, spinup: 0.0, sfx, mesh, muzzle, tracer: TRACER, two_handed,
    }
}

pub const SIDEARM: usize = 0;

pub static WEAPONS: [WeaponDef; 18] = [
    hitscan("P9 SIDEARM", "RELIABLE. NEVER RUNS DRY.", 1, 0.03, 0, 34.0, 5.5, 14, u32::MAX, 1.05, 0, 0, 1, 0.08, 0.04, 2.0, 0.1, Sfx::Pistol, m::GUN0, 0.34, false),
    hitscan("VIPER SMG", "BUZZING HOSE OF LEAD.", 1, 0.075, 0, 19.0, 14.0, 34, 340, 1.45, 300, 60, 1, 0.05, 0.03, 1.2, 0.1, Sfx::Smg, m::GUN1, 0.44, false),
    hitscan("VANGUARD AR", "THE DEPENDABLE ALL-ROUNDER.", 1, 0.04, 1, 31.0, 10.0, 30, 300, 1.8, 550, 80, 1, 0.09, 0.05, 2.0, 0.2, Sfx::Rifle, m::GUN2, 0.78, true),
    hitscan("BREACHER PUMP", "OPENS DOORS. AND TORSOS.", 10, 0.17, 0, 24.0, 1.3, 6, 54, 2.5, 650, 70, 1, 0.4, 0.2, 5.0, 0.8, Sfx::Shotgun, m::GUN3, 0.8, true),
    hitscan("DRAGOON MAGNUM", "PUNCHES CLEAN THROUGH.", 1, 0.012, 3, 125.0, 2.5, 6, 66, 2.0, 500, 70, 1, 0.3, 0.16, 5.0, 0.7, Sfx::Magnum, m::GUN15, 0.34, false),
    hitscan("TWIN STINGERS", "TWO GUNS. NO PATIENCE.", 1, 0.11, 0, 17.0, 25.0, 64, 512, 1.9, 900, 90, 2, 0.05, 0.03, 1.1, 0.1, Sfx::Smg, m::GUN16, 0.3, true),
    hitscan("DRUMFIRE", "FULL-AUTO SHOTGUN MAYHEM.", 7, 0.15, 0, 18.0, 4.6, 20, 140, 2.6, 1400, 110, 3, 0.26, 0.14, 3.0, 0.7, Sfx::Shotgun, m::GUN4, 0.76, true),
    hitscan("LONGSHOT", "ONE BULLET, EIGHT CORPSES.", 1, 0.002, 8, 340.0, 1.05, 5, 45, 2.7, 1200, 100, 3, 0.45, 0.26, 7.0, 0.9, Sfx::Sniper, m::GUN5, 1.15, true),
    hitscan("WARDOG LMG", "100 ROUNDS OF SUPPRESSION.", 1, 0.06, 1, 38.0, 11.0, 100, 500, 4.2, 1800, 150, 4, 0.11, 0.05, 2.5, 0.3, Sfx::Lmg, m::GUN6, 0.98, true),
    WeaponDef { name: "THUMPER", blurb: "LOBS BOUNCING GRENADES.", fire: Fire::Explosive { speed: 22.0, radius: 4.6, arc: true }, damage: 190.0, rate: 1.7, mag: 6, reserve: 36, reload: 2.9, price: 1700, ammo_price: 170, unlock_wave: 4, kick: 0.2, recoil: 0.15, knock: 3.0, gore: 1.0, spinup: 0.0, sfx: Sfx::Launcher, mesh: m::GUN8, muzzle: 0.62, tracer: TRACER, two_handed: true },
    WeaponDef { name: "CREMATOR", blurb: "THE CROWD GOES WILD.", fire: Fire::Flame, damage: 9.0, rate: 28.0, mag: 220, reserve: 880, reload: 3.0, price: 1600, ammo_price: 140, unlock_wave: 4, kick: 0.012, recoil: 0.0, knock: 0.15, gore: 0.0, spinup: 0.0, sfx: Sfx::Flame, mesh: m::GUN10, muzzle: 0.82, tracer: TRACER, two_handed: true },
    WeaponDef { name: "FROSTBITE", blurb: "FREEZE. SHATTER. REPEAT.", fire: Fire::Cryo, damage: 6.0, rate: 28.0, mag: 200, reserve: 800, reload: 2.8, price: 1300, ammo_price: 120, unlock_wave: 3, kick: 0.01, recoil: 0.0, knock: 0.05, gore: 0.1, spinup: 0.0, sfx: Sfx::Cryo, mesh: m::GUN17, muzzle: 0.72, tracer: TRACER, two_handed: true },
    WeaponDef { name: "ARC CASTER", blurb: "LIGHTNING JUMPS THE HORDE.", fire: Fire::Tesla { chains: 7 }, damage: 52.0, rate: 5.0, mag: 40, reserve: 240, reload: 2.4, price: 2200, ammo_price: 180, unlock_wave: 5, kick: 0.09, recoil: 0.04, knock: 1.0, gore: 0.15, spinup: 0.0, sfx: Sfx::Tesla, mesh: m::GUN11, muzzle: 0.68, tracer: [2.0, 4.5, 9.0], two_handed: true },
    WeaponDef { name: "RIPPER", blurb: "SAWBLADES THAT COME BACK.", fire: Fire::Saw { bounces: 8 }, damage: 75.0, rate: 3.0, mag: 12, reserve: 84, reload: 2.5, price: 2000, ammo_price: 170, unlock_wave: 5, kick: 0.12, recoil: 0.08, knock: 1.5, gore: 1.0, spinup: 0.0, sfx: Sfx::Saw, mesh: m::GUN13, muzzle: 0.6, tracer: TRACER, two_handed: true },
    WeaponDef { name: "WIDOWMAKER", blurb: "ROCKET AIRMAIL.", fire: Fire::Explosive { speed: 34.0, radius: 6.8, arc: false }, damage: 460.0, rate: 1.05, mag: 4, reserve: 22, reload: 3.1, price: 3000, ammo_price: 260, unlock_wave: 6, kick: 0.4, recoil: 0.3, knock: 4.0, gore: 1.0, spinup: 0.0, sfx: Sfx::Rocket, mesh: m::GUN9, muzzle: 0.95, tracer: TRACER, two_handed: true },
    WeaponDef { name: "SOLAR LANCE", blurb: "A BEAM OF PURE SPITE.", fire: Fire::Beam, damage: 9.0, rate: 50.0, mag: 300, reserve: 1200, reload: 3.0, price: 3500, ammo_price: 280, unlock_wave: 7, kick: 0.012, recoil: 0.0, knock: 0.3, gore: 0.45, spinup: 0.0, sfx: Sfx::Beam, mesh: m::GUN12, muzzle: 0.84, tracer: [14.0, 4.0, 1.5], two_handed: true },
    WeaponDef { name: "MEATGRINDER", blurb: "SPIN UP. EVERYTHING DIES.", fire: Fire::Hitscan { pellets: 1, spread: 0.085, pierce: 1 }, damage: 27.0, rate: 34.0, mag: 300, reserve: 1200, reload: 4.6, price: 4000, ammo_price: 300, unlock_wave: 8, kick: 0.07, recoil: 0.02, knock: 1.6, gore: 0.45, spinup: 0.75, sfx: Sfx::Minigun, mesh: m::GUN7, muzzle: 1.04, tracer: TRACER, two_handed: true },
    hitscan("GAUSS RAIL", "NOTHING STOPS IT. NOTHING.", 1, 0.0, 999, 950.0, 0.85, 3, 21, 3.0, 5000, 380, 10, 0.6, 0.35, 9.0, 1.0, Sfx::Rail, m::GUN14, 1.0, true),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZKind {
    Walker,
    Runner,
    Crawler,
    Brute,
    Bloater,
    Spitter,
    Screamer,
    Titan,
}

impl ZKind {
    pub fn from_u8(v: u8) -> Option<ZKind> {
        use ZKind::*;
        [Walker, Runner, Crawler, Brute, Bloater, Spitter, Screamer, Titan].get(v as usize).copied()
    }
}

pub struct ZombieDef {
    pub hp: f32,
    pub speed: f32,
    pub damage: f32,
    pub struct_damage: f32,
    pub attack_time: f32,
    pub reach: f32,
    pub radius: f32,
    pub scale: f32,
    pub scrap: u32,
    pub first_wave: u32,
    pub weight: f32,
    pub cost: f32,
}

pub fn zdef(k: ZKind) -> ZombieDef {
    let d = |hp, speed, damage, struct_damage, attack_time, reach, radius, scale, scrap, first_wave, weight, cost| ZombieDef {
        hp, speed, damage, struct_damage, attack_time, reach, radius, scale, scrap, first_wave, weight, cost,
    };
    match k {
        ZKind::Walker => d(70.0, 2.1, 12.0, 14.0, 0.9, 1.15, 0.36, 1.0, 5, 1, 10.0, 1.0),
        ZKind::Runner => d(45.0, 5.2, 8.0, 9.0, 0.55, 1.1, 0.32, 0.94, 7, 2, 5.0, 1.3),
        ZKind::Crawler => d(40.0, 3.6, 9.0, 8.0, 0.6, 1.0, 0.34, 0.92, 6, 3, 3.0, 1.0),
        ZKind::Bloater => d(160.0, 1.7, 8.0, 20.0, 1.0, 1.2, 0.5, 1.15, 14, 4, 2.0, 3.0),
        ZKind::Spitter => d(85.0, 2.5, 14.0, 22.0, 2.4, 14.0, 0.36, 1.02, 15, 5, 1.6, 3.5),
        ZKind::Brute => d(620.0, 1.75, 34.0, 95.0, 1.3, 1.75, 0.68, 1.5, 45, 6, 1.1, 9.0),
        ZKind::Screamer => d(115.0, 3.3, 7.0, 6.0, 0.8, 1.1, 0.34, 0.98, 22, 7, 0.55, 6.0),
        ZKind::Titan => d(7000.0, 1.8, 60.0, 300.0, 1.6, 3.1, 1.45, 3.1, 500, 5, 0.0, 60.0),
    }
}

pub const SPAWN_KINDS: [ZKind; 7] =
    [ZKind::Walker, ZKind::Runner, ZKind::Crawler, ZKind::Bloater, ZKind::Spitter, ZKind::Brute, ZKind::Screamer];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SKind {
    WoodWall,
    SteelWall,
    Gate,
    Spikes,
    Wire,
    Mine,
    SentryMg,
    SentryShotgun,
    SentryFlame,
    SentryTesla,
    SentryMini,
    SentryRocket,
    SentryMortar,
    Floodlight,
    AmmoDepot,
    RepairBay,
    Core,
}

pub const BUILDABLE: [SKind; 16] = [
    SKind::WoodWall, SKind::SteelWall, SKind::Gate, SKind::Spikes, SKind::Wire, SKind::Mine,
    SKind::SentryMg, SKind::SentryShotgun, SKind::SentryFlame, SKind::SentryTesla, SKind::SentryMini,
    SKind::SentryRocket, SKind::SentryMortar, SKind::Floodlight, SKind::AmmoDepot, SKind::RepairBay,
];

impl SKind {
    pub fn from_u8(v: u8) -> Option<SKind> {
        if v as usize == BUILDABLE.len() {
            Some(SKind::Core)
        } else {
            BUILDABLE.get(v as usize).copied()
        }
    }
}

pub struct TurretDef {
    pub range: f32,
    pub rate: f32,
    pub damage: f32,
    pub min_range: f32,
    pub turn: f32,
    pub muzzle: f32,
}

pub struct StructDef {
    pub name: &'static str,
    pub blurb: &'static str,
    pub cost: u32,
    pub hp: f32,
    pub blocks: bool,
    pub mesh: usize,
    pub head: Option<usize>,
    pub turret: Option<TurretDef>,
    pub nav_cost: u32,
    pub unlock_wave: u32,
}

pub fn sdef(k: SKind) -> StructDef {
    let plain = |name, blurb, cost, hp, blocks, mesh, nav_cost, unlock_wave| StructDef {
        name, blurb, cost, hp, blocks, mesh, head: None, turret: None, nav_cost, unlock_wave,
    };
    #[allow(clippy::too_many_arguments)]
    let tur = |name, blurb, cost, hp, head, range, rate, damage, min_range, turn, muzzle, unlock_wave| StructDef {
        name, blurb, cost, hp, blocks: true, mesh: m::SENTRY_BASE, head: Some(head),
        turret: Some(TurretDef { range, rate, damage, min_range, turn, muzzle }), nav_cost: 120, unlock_wave,
    };
    match k {
        SKind::WoodWall => plain("WOOD WALL", "CHEAP. HOLDS FOR A WHILE.", 20, 320.0, true, m::WALL_WOOD, 70, 1),
        SKind::SteelWall => plain("STEEL WALL", "SERIOUS PROTECTION.", 70, 1300.0, true, m::WALL_STEEL, 160, 2),
        SKind::Gate => plain("GATE", "YOU PASS. THEY DO NOT.", 60, 800.0, true, m::GATE, 120, 1),
        SKind::Spikes => plain("SPIKE TRAP", "SHREDS WHATEVER WALKS OVER.", 45, 500.0, false, m::SPIKES, 16, 1),
        SKind::Wire => plain("RAZOR WIRE", "SLOWS THE HORDE, CUTS THEM.", 30, 260.0, false, m::WIRE, 22, 1),
        SKind::Mine => plain("LAND MINE", "ONE BIG BANG.", 40, 60.0, false, m::MINE, 0, 1),
        SKind::SentryMg => tur("MG SENTRY", "TWIN GUNS, ALWAYS WATCHING.", 180, 340.0, m::TURRET_MG, 17.0, 8.0, 19.0, 0.0, 6.0, 1.15, 1),
        SKind::SentryShotgun => tur("SCATTER SENTRY", "SHREDS ANYTHING CLOSE.", 300, 420.0, m::TURRET_SHOTGUN, 9.5, 1.5, 21.0, 0.0, 7.0, 1.0, 2),
        SKind::SentryFlame => tur("FLAME SENTRY", "A WALL OF FIRE.", 380, 460.0, m::TURRET_FLAME, 7.5, 26.0, 7.0, 0.0, 4.0, 0.95, 3),
        SKind::SentryTesla => tur("TESLA COIL", "CHAIN LIGHTNING. NO AIM.", 600, 380.0, m::TURRET_TESLA, 11.0, 1.7, 62.0, 0.0, 30.0, 0.65, 4),
        SKind::SentryMini => tur("GATLING SENTRY", "SIX BARRELS OF NOPE.", 900, 520.0, m::TURRET_MINI, 20.0, 24.0, 15.0, 0.0, 3.5, 1.26, 6),
        SKind::SentryRocket => tur("ROCKET SENTRY", "LONG RANGE CROWD CONTROL.", 800, 420.0, m::TURRET_ROCKET, 24.0, 0.8, 210.0, 3.0, 3.0, 0.55, 5),
        SKind::SentryMortar => tur("MORTAR", "RAINS DEATH ACROSS THE MAP.", 1000, 420.0, m::TURRET_MORTAR, 38.0, 0.55, 320.0, 8.0, 3.0, 1.0, 7),
        SKind::Floodlight => plain("FLOOD LIGHT", "PUSHES BACK THE DARK.", 40, 150.0, true, m::FLOODLIGHT, 70, 1),
        SKind::AmmoDepot => plain("AMMO DEPOT", "STAND NEARBY TO REFILL.", 260, 420.0, true, m::AMMO_DEPOT, 110, 2),
        SKind::RepairBay => plain("REPAIR BAY", "SLOWLY MENDS NEARBY BUILDS.", 420, 420.0, true, m::REPAIR_BAY, 110, 3),
        SKind::Core => plain("REACTOR", "IF THIS DIES, IT IS OVER.", 0, 6000.0, true, m::CORE, 500, 1),
    }
}

/// How much each sentry level multiplies damage and fire rate.
pub const LEVEL_DMG: [f32; 3] = [1.0, 1.55, 2.3];
pub const LEVEL_RATE: [f32; 3] = [1.0, 1.15, 1.35];
pub const LEVEL_COST: [f32; 3] = [0.0, 0.8, 1.6];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tables_are_sane() {
        assert_eq!(WEAPONS.len(), 18);
        let mut names = std::collections::HashSet::new();
        for w in &WEAPONS {
            assert!(names.insert(w.name), "duplicate gun {}", w.name);
            assert!(w.rate > 0.0 && w.mag > 0 && w.damage > 0.0);
            assert!(w.name.len() <= 16);
        }
        for k in BUILDABLE {
            let d = sdef(k);
            assert!(d.cost > 0 && d.hp > 0.0, "{}", d.name);
        }
        for k in SPAWN_KINDS {
            let d = zdef(k);
            assert!(d.weight > 0.0 && d.cost > 0.0);
        }
    }
}
