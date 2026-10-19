# Independent Python Empirical Inspector for Milestone 3 Exports
import os
import sys
import xml.etree.ElementTree as ET

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")

def check(name, cond, msg=""):
    status = "[PASS]" if cond else "[FAIL]"
    print(f"  {status} {name}: {msg}")
    if not cond:
        return False
    return True

def main():
    build_dir = sys.argv[1] if len(sys.argv) > 1 else "studio/build-msvc"
    print(f"=== Independent Python Export Inspector ({build_dir}) ===")
    all_ok = True

    # 1. 14-Layer SVG
    svg_14_path = os.path.join(build_dir, "test_14layers_structure.svg")
    if os.path.exists(svg_14_path):
        tree = ET.parse(svg_14_path)
        root = tree.getroot()
        all_ok &= check("SVG 14-layer root tag", root.tag.endswith("svg"), root.tag)
        all_ok &= check("SVG width in mm", root.get("width") == "210mm", root.get("width"))
        all_ok &= check("SVG height in mm", root.get("height") == "297mm", root.get("height"))

        # Check layers
        expected_layers = [
            "grid", "shade", "calibration", "reconstruction", "inner",
            "refline.center", "refline.rim", "refline.base", "outline", "section",
            "break", "dimension", "scalebar", "label"
        ]
        found_layers = [elem.get("id") for elem in root.iter() if elem.get("id") in expected_layers]
        all_ok &= check("SVG 14 layers count", len(found_layers) == 14, f"found {len(found_layers)}")
        all_ok &= check("SVG 14 layers order", found_layers == expected_layers, str(found_layers))
    else:
        all_ok &= check("SVG 14-layer file exists", False, svg_14_path)

    # 2. DXF R12
    dxf_path = os.path.join(build_dir, "test_cad_r12.dxf")
    if os.path.exists(dxf_path):
        with open(dxf_path, "r", encoding="utf-8", errors="ignore") as f:
            dxf_text = f.read()
        all_ok &= check("DXF $ACADVER AC1009 (R12)", "$ACADVER\n1\nAC1009" in dxf_text)
        all_ok &= check("DXF $INSUNITS 4 (mm)", "$INSUNITS\n70\n4" in dxf_text)
        
        expected_dxf_layers = [
            "01_AXIS_CENTER", "02_EXTERIOR_OUTLINE", "03_EXTERIOR_FEATURES",
            "04_SECTION_WALL", "05_SECTION_HATCH", "06_INTERIOR_OUTLINE",
            "07_PROJECTION_RIM", "08_PROJECTION_BASE", "09_DIMENSION_MEASURE",
            "10_ANNOTATION_TEXT", "11_SCALE_BAR"
        ]
        for ly in expected_dxf_layers:
            all_ok &= check(f"DXF Layer {ly}", f"2\n{ly}\n" in dxf_text)
        
        all_ok &= check("DXF ends with 0\\nEOF", dxf_text.strip().endswith("0\nEOF"))
        all_ok &= check("DXF no NaN values", "nan" not in dxf_text.lower() and "inf" not in dxf_text.lower())
    else:
        all_ok &= check("DXF file exists", False, dxf_path)

    # 3. PDF 600 DPI
    pdf_path = os.path.join(build_dir, "test_print_600dpi.pdf")
    if os.path.exists(pdf_path):
        size = os.path.getsize(pdf_path)
        all_ok &= check("PDF file size substantial", size > 2048, f"{size} bytes")
        with open(pdf_path, "rb") as f:
            head = f.read(16)
        all_ok &= check("PDF magic header", head.startswith(b"%PDF-1."), str(head))
    else:
        all_ok &= check("PDF file exists", False, pdf_path)

    # 4. PNG 300 DPI
    png_path = os.path.join(build_dir, "test_raster_300dpi.png")
    if os.path.exists(png_path):
        size = os.path.getsize(png_path)
        all_ok &= check("PNG file size substantial", size > 10000, f"{size} bytes")
        with open(png_path, "rb") as f:
            png_bytes = f.read()
        all_ok &= check("PNG magic header", png_bytes.startswith(b"\x89PNG\r\n\x1a\n"))
        # Parse IHDR chunk
        import struct
        w, h = struct.unpack(">II", png_bytes[16:24])
        all_ok &= check("PNG dimensions 300 DPI A4", abs(w - 2480) <= 2 and abs(h - 3507) <= 2, f"{w}x{h}")
        # Parse pHYs chunk if present
        phys_pos = png_bytes.find(b"pHYs")
        if phys_pos != -1:
            ppm_x, ppm_y, unit = struct.unpack(">IIB", png_bytes[phys_pos+4:phys_pos+13])
            all_ok &= check("PNG pHYs resolution (11811 dpm = 300 dpi)", 11810 <= ppm_x <= 11812 and unit == 1, f"{ppm_x} dpm, unit={unit}")
    else:
        all_ok &= check("PNG file exists", False, png_path)

    # 5. Neck line & Break mark SVG details
    neck_svg = os.path.join(build_dir, "test_neck_layer.svg")
    if os.path.exists(neck_svg):
        with open(neck_svg, "r", encoding="utf-8") as f:
            content = f.read()
        all_ok &= check("Neck exterior solid & interior dash (12,4)", 'stroke-dasharray="12,4"' in content)
    
    break_svg = os.path.join(build_dir, "test_break_sherd.svg")
    if os.path.exists(break_svg):
        with open(break_svg, "r", encoding="utf-8") as f:
            content = f.read()
        all_ok &= check("Break marks stroke width 0.13", 'stroke-width="0.13"' in content)

    # 6. Sherd mode missing base blank principle
    blank_svg = os.path.join(build_dir, "test_blank_base_14.svg")
    if os.path.exists(blank_svg):
        with open(blank_svg, "r", encoding="utf-8") as f:
            content = f.read()
        all_ok &= check("Sherd mode title has '저경 — mm'", "저경 — mm" in content)

    # 7. 100mm calibration square
    calib_svg = os.path.join(build_dir, "test_calib_true.svg")
    if os.path.exists(calib_svg):
        with open(calib_svg, "r", encoding="utf-8") as f:
            content = f.read()
        all_ok &= check("Calibration square 100x100 mm at (20, 182)", '<rect x="20" y="182" width="100" height="100"/>' in content)
        all_ok &= check("Calibration text CALIBRATION 100 × 100 mm", "CALIBRATION 100 × 100 mm" in content)

    print("\nSummary: " + ("ALL VERIFICATIONS PASSED" if all_ok else "VERIFICATION FAILED"))
    return 0 if all_ok else 1

if __name__ == "__main__":
    sys.exit(main())
