"""A fake of dsh-visual's build.py: build.py <src-dir> <out.html> --repo <root> --rev <rev>."""
import pathlib, sys

src, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
args = sys.argv[3:]
if "--repo" not in args or "--rev" not in args:
    sys.exit("build failed: --repo and --rev are required")
template = (src / "template.html").read_text()
if "BUILD-FAIL" in template:
    sys.exit("build failed: page.html: no line after 0 contains 'BUILD-FAIL'")
if "{{" in template:
    sys.exit("build failed: leftover placeholder")
out.write_text(template)
(src / "page.html").write_text(template)
print(f"ok {out} (0 sources)")
