//! analiza-rumenire
//!
//! Citește fotografiile feliilor de măr dintr-un folder, calculează
//! luminozitatea medie L* (0 = negru, 100 = alb) în centrul fiecărei
//! poze și scrie rezultatele într-un fișier CSV pe care îl citește
//! graficul din lucrare.
//!
//! Numele pozelor trebuie să fie de forma:
//!     <tratament>_z<zi>_f<felie>.jpg     ex.: lamaie_z3_f2.jpg
//! Partea "_f<felie>" poate lipsi dacă aveți o singură felie.
//!
//! Utilizare:
//!     cargo run --release -- <folder_poze> [--iesire lstar.csv] [--roi 0.4]

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

/// Setările programului, citite din linia de comandă.
struct Setari {
    folder: PathBuf,
    iesire: PathBuf,
    /// Latura pătratului din centru, ca fracție din latura mică a pozei.
    roi: f64,
}

/// O fotografie recunoscută după nume.
struct Poza {
    cale: PathBuf,
    tratament: String,
    zi: u32,
}

fn main() {
    let setari = citeste_argumente().unwrap_or_else(|mesaj| {
        eprintln!("Eroare: {mesaj}");
        eprintln!("Utilizare: analiza-rumenire <folder_poze> [--iesire lstar.csv] [--roi 0.4]");
        process::exit(1);
    });

    let poze = gaseste_poze(&setari.folder).unwrap_or_else(|e| {
        eprintln!("Nu pot citi folderul {}: {e}", setari.folder.display());
        process::exit(1);
    });
    if poze.is_empty() {
        eprintln!("Nu am găsit nicio poză de forma <tratament>_z<zi>_f<felie>.jpg");
        process::exit(1);
    }

    // rezultate[zi][tratament] = lista valorilor L*, câte una pe felie
    let mut rezultate: BTreeMap<u32, BTreeMap<String, Vec<f64>>> = BTreeMap::new();

    for poza in &poze {
        match luminozitate_medie(&poza.cale, setari.roi) {
            Ok(l) => {
                println!("{:<30} L* = {:6.2}", poza.cale.file_name().unwrap().to_string_lossy(), l);
                rezultate
                    .entry(poza.zi)
                    .or_default()
                    .entry(poza.tratament.clone())
                    .or_default()
                    .push(l);
            }
            Err(e) => eprintln!("Sar peste {}: {e}", poza.cale.display()),
        }
    }

    if let Err(e) = scrie_csv(&setari.iesire, &rezultate) {
        eprintln!("Nu pot scrie {}: {e}", setari.iesire.display());
        process::exit(1);
    }
    println!("\nRezultatele au fost salvate în {}", setari.iesire.display());
}

fn citeste_argumente() -> Result<Setari, String> {
    let mut args = env::args().skip(1);
    let mut folder = None;
    let mut iesire = PathBuf::from("lstar.csv");
    let mut roi = 0.4;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iesire" => {
                iesire = PathBuf::from(args.next().ok_or("lipsește numele fișierului după --iesire")?);
            }
            "--roi" => {
                let valoare = args.next().ok_or("lipsește valoarea după --roi")?;
                roi = valoare
                    .parse()
                    .map_err(|_| format!("valoare --roi invalidă: {valoare}"))?;
                if !(0.05..=1.0).contains(&roi) {
                    return Err("--roi trebuie să fie între 0.05 și 1.0".into());
                }
            }
            _ if folder.is_none() => folder = Some(PathBuf::from(arg)),
            _ => return Err(format!("argument necunoscut: {arg}")),
        }
    }

    Ok(Setari {
        folder: folder.ok_or("lipsește folderul cu poze")?,
        iesire,
        roi,
    })
}

/// Caută în folder pozele cu nume valid și le sortează după nume.
fn gaseste_poze(folder: &Path) -> std::io::Result<Vec<Poza>> {
    let mut poze = Vec::new();
    for intrare in fs::read_dir(folder)? {
        let cale = intrare?.path();
        let extensie = cale
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        if !matches!(extensie.as_deref(), Some("jpg" | "jpeg" | "png")) {
            continue;
        }
        let nume = cale.file_stem().and_then(|n| n.to_str()).unwrap_or("");
        if let Some((tratament, zi)) = interpreteaza_nume(nume) {
            poze.push(Poza { cale, tratament, zi });
        }
    }
    poze.sort_by(|a, b| a.cale.cmp(&b.cale));
    Ok(poze)
}

/// "lamaie_z3_f2" -> ("lamaie", 3). Returnează None dacă numele nu se potrivește.
fn interpreteaza_nume(nume: &str) -> Option<(String, u32)> {
    let mut parti = nume.split('_');
    let tratament = parti.next()?.to_string();
    let zi = parti.next()?.strip_prefix('z')?.parse().ok()?;
    match parti.next() {
        None => {}
        Some(felie) if felie.strip_prefix('f')?.parse::<u32>().is_ok() => {}
        Some(_) => return None,
    }
    if tratament.is_empty() || parti.next().is_some() {
        return None;
    }
    Some((tratament, zi))
}

/// Media L* într-un pătrat din centrul pozei.
fn luminozitate_medie(cale: &Path, roi: f64) -> Result<f64, image::ImageError> {
    let img = image::open(cale)?.to_rgb8();
    let (latime, inaltime) = img.dimensions();

    let latura = ((latime.min(inaltime) as f64) * roi).round().max(1.0) as u32;
    let x0 = (latime - latura) / 2;
    let y0 = (inaltime - latura) / 2;

    let mut suma = 0.0;
    for y in y0..y0 + latura {
        for x in x0..x0 + latura {
            let [r, g, b] = img.get_pixel(x, y).0;
            suma += l_star(r, g, b);
        }
    }
    Ok(suma / (latura as f64 * latura as f64))
}

/// Luminozitatea L* din CIELAB pentru un pixel sRGB (iluminant D65).
/// L* depinde doar de componenta Y (luminanța) a culorii.
fn l_star(r: u8, g: u8, b: u8) -> f64 {
    // 1. Din valori 0–255 „cu gamma” în valori liniare 0–1.
    fn liniar(c: u8) -> f64 {
        let c = c as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    // 2. Luminanța relativă Y (alb = 1).
    let y = 0.2126 * liniar(r) + 0.7152 * liniar(g) + 0.0722 * liniar(b);

    // 3. Formula standard CIE pentru L*.
    let delta: f64 = 6.0 / 29.0;
    let f = if y > delta.powi(3) {
        y.cbrt()
    } else {
        y / (3.0 * delta * delta) + 4.0 / 29.0
    };
    116.0 * f - 16.0
}

/// Media și abaterea standard (de selecție, n - 1).
fn medie_si_abatere(valori: &[f64]) -> (f64, f64) {
    let n = valori.len() as f64;
    let medie = valori.iter().sum::<f64>() / n;
    if valori.len() < 2 {
        return (medie, 0.0);
    }
    let varianta = valori.iter().map(|v| (v - medie).powi(2)).sum::<f64>() / (n - 1.0);
    (medie, varianta.sqrt())
}

/// Scrie CSV-ul în formatul citit de lucrare:
///     zi,control,control_sd,lamaie,lamaie_sd
/// Tratamentul "control" este pus primul, celelalte în ordine alfabetică.
fn scrie_csv(
    cale: &Path,
    rezultate: &BTreeMap<u32, BTreeMap<String, Vec<f64>>>,
) -> std::io::Result<()> {
    let mut tratamente: Vec<&String> = rezultate.values().flat_map(|t| t.keys()).collect();
    tratamente.sort_by_key(|t| (t.as_str() != "control", t.as_str()));
    tratamente.dedup();

    let mut text = String::from("zi");
    for t in &tratamente {
        text.push_str(&format!(",{t},{t}_sd"));
    }
    text.push('\n');

    for (zi, pe_tratament) in rezultate {
        text.push_str(&zi.to_string());
        for t in &tratamente {
            match pe_tratament.get(*t) {
                Some(valori) => {
                    let (medie, sd) = medie_si_abatere(valori);
                    text.push_str(&format!(",{medie:.1},{sd:.1}"));
                }
                // Lipsă: pgfplots sare peste valorile "nan".
                None => text.push_str(",nan,nan"),
            }
        }
        text.push('\n');
    }

    if let Some(parinte) = cale.parent() {
        if !parinte.as_os_str().is_empty() {
            fs::create_dir_all(parinte)?;
        }
    }
    fs::write(cale, text)
}

#[cfg(test)]
mod teste {
    use super::*;

    #[test]
    fn alb_si_negru() {
        assert!((l_star(255, 255, 255) - 100.0).abs() < 1e-6);
        assert!(l_star(0, 0, 0).abs() < 1e-6);
    }

    #[test]
    fn gri_mediu() {
        // sRGB 119 are L* ≈ 50 (valoare de referință cunoscută)
        assert!((l_star(119, 119, 119) - 50.0).abs() < 0.5);
    }

    #[test]
    fn nume_de_fisiere() {
        assert_eq!(interpreteaza_nume("lamaie_z3_f2"), Some(("lamaie".into(), 3)));
        assert_eq!(interpreteaza_nume("control_z0"), Some(("control".into(), 0)));
        assert_eq!(interpreteaza_nume("poza_mea"), None);
        assert_eq!(interpreteaza_nume("lamaie_z3_x"), None);
    }

    #[test]
    fn statistica() {
        let (m, sd) = medie_si_abatere(&[2.0, 4.0, 6.0]);
        assert!((m - 4.0).abs() < 1e-9);
        assert!((sd - 2.0).abs() < 1e-9);
    }
}
