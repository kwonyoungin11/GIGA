use crate::plate_composer::ComposedPlate2D;
use crate::svg_illustrator::render_plate_to_illustrator_svg;
use arch3d_core::{Arch3dError, StacklessLbvh, TriMesh};
use arch3d_shade::{render_orthographic_rubbing_image, RubbingRenderParams};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RetPackageManifest {
    pub output_dir: PathBuf,
    pub cutline_fc0_svg: PathBuf,
    pub cutline_rc0_svg: PathBuf,
    pub cutline_tc0_svg: PathBuf,
    pub rubbing_ext_png: PathBuf,
    pub rubbing_int_png: PathBuf,
    pub rubbing_bot_png: PathBuf,
    pub rubbing_meta_txt: PathBuf,
}

/// 발굴조사 보고서 표준 `ret/` 폴더 패키지를 일괄 생성한다 (`cutline_fc0/rc0/tc0.svg` + `rubbing_sc0_*.png/.txt`).
pub fn export_ret_folder_package(
    output_dir: impl AsRef<Path>,
    plate: &ComposedPlate2D,
    mesh: &TriMesh,
    bvh: Option<&StacklessLbvh>,
    rubbing_params: RubbingRenderParams,
) -> Result<RetPackageManifest, Arch3dError> {
    let dir = output_dir.as_ref();
    fs::create_dir_all(dir).map_err(|e| Arch3dError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    })?;

    let svg_str = render_plate_to_illustrator_svg(plate);
    let cutline_fc0_svg = dir.join("cutline_fc0.svg");
    let cutline_rc0_svg = dir.join("cutline_rc0.svg");
    let cutline_tc0_svg = dir.join("cutline_tc0.svg");

    for target in [&cutline_fc0_svg, &cutline_rc0_svg, &cutline_tc0_svg] {
        fs::write(target, svg_str.as_bytes()).map_err(|e| Arch3dError::Io {
            path: target.display().to_string(),
            message: e.to_string(),
        })?;
    }

    let img_w = ((plate.canvas_width_mm * 5.0).round() as u32).clamp(128, 2048);
    let img_h = ((plate.canvas_height_mm * 5.0).round() as u32).clamp(128, 2048);
    let rubbing_img = render_orthographic_rubbing_image(mesh, bvh, rubbing_params, img_w, img_h);

    let rubbing_ext_png = dir.join("rubbing_sc0_ext.png");
    let rubbing_int_png = dir.join("rubbing_sc0_int.png");
    let rubbing_bot_png = dir.join("rubbing_sc0_bot.png");

    for png_path in [&rubbing_ext_png, &rubbing_int_png, &rubbing_bot_png] {
        rubbing_img.save(png_path).map_err(|e| Arch3dError::Io {
            path: png_path.display().to_string(),
            message: e.to_string(),
        })?;
    }

    let rubbing_meta_txt = dir.join("rubbing_sc0_meta.txt");
    let meta_body = format!(
        "title={}\nmode={:?}\nscale={}\ncanvas_mm={:.2}x{:.2}\npx_per_mm=5.0\nalgorithm={}\nink_density_pct={:.1}\ncurv_radius_mm={:.2}\n",
        plate.title,
        plate.mode,
        plate.scale_preset.label(),
        plate.canvas_width_mm,
        plate.canvas_height_mm,
        rubbing_params.algorithm.label_kr(),
        rubbing_params.ink_density_pct,
        rubbing_params.curv_radius_mm
    );
    fs::write(&rubbing_meta_txt, meta_body.as_bytes()).map_err(|e| Arch3dError::Io {
        path: rubbing_meta_txt.display().to_string(),
        message: e.to_string(),
    })?;

    Ok(RetPackageManifest {
        output_dir: dir.to_path_buf(),
        cutline_fc0_svg,
        cutline_rc0_svg,
        cutline_tc0_svg,
        rubbing_ext_png,
        rubbing_int_png,
        rubbing_bot_png,
        rubbing_meta_txt,
    })
}
