use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bivar_poc_text_parley::cpu;
use bivar_poc_text_parley::fonts::TestFont;
use bivar_poc_text_parley::image::Rgba;
use bivar_poc_text_parley::page::{self, Page};
use bivar_poc_text_parley::shaping::Shaper;

const USAGE: &str = "Uso:
  bivar-poc-text-parley              grava as imagens com e sem placa de vídeo
  bivar-poc-text-parley --sem-placa  grava só as imagens sem placa de vídeo";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let with_gpu = match args.as_slice() {
        [] => true,
        [flag] if flag == "--sem-placa" => false,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match export(with_gpu) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn export(with_gpu: bool) -> Result<(), String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("exportacoes");
    let mut shaper = Shaper::new();
    let mut pages: Vec<(String, Page)> = TestFont::ALL
        .into_iter()
        .map(|font| {
            (
                font.slug().to_string(),
                page::criteria_page(&mut shaper, font),
            )
        })
        .collect();
    pages.push(("paragrafo".to_string(), page::paragraph_page(&mut shaper)));

    println!("Sem placa de vídeo: vello_cpu, modo {}", cpu::simd_level());
    for (name, page) in &pages {
        save(&cpu::render(page), &dir, &format!("cpu_{name}.png"))?;
    }
    if with_gpu {
        export_gpu(&pages, &dir)?;
    }
    println!("Imagens em {}", dir.display());
    Ok(())
}

#[cfg(feature = "gpu")]
fn export_gpu(pages: &[(String, Page)], dir: &Path) -> Result<(), String> {
    let mut gpu = bivar_poc_text_parley::gpu::Headless::new()?;
    println!("Com placa de vídeo: {}", gpu.adapter_name);
    for (name, page) in pages {
        save(&gpu.render(page)?, dir, &format!("gpu_{name}.png"))?;
    }
    Ok(())
}

#[cfg(not(feature = "gpu"))]
fn export_gpu(_: &[(String, Page)], _: &Path) -> Result<(), String> {
    Err("compilado sem a placa de vídeo; use --sem-placa".to_string())
}

fn save(image: &Rgba, dir: &Path, name: &str) -> Result<(), String> {
    let path = dir.join(name);
    image.save_png(&path)?;
    println!("  {name}");
    Ok(())
}
