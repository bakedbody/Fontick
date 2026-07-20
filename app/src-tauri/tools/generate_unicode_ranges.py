from pathlib import Path
from urllib.request import Request, urlopen


UNICODE_VERSION = "17.0.0"
BASE_URL = f"https://www.unicode.org/Public/{UNICODE_VERSION}/ucd"
FILES = (
    "UnicodeData.txt",
    "Scripts.txt",
    "ScriptExtensions.txt",
    "PropertyValueAliases.txt",
)

CODE_POINT_COUNT = 0x110000
OUTPUT_PATH = Path(__file__).resolve().parents[1] / "src" / "unicode_ranges.rs"


def download(filename: str) -> str:
    request = Request(f"{BASE_URL}/{filename}", headers={"User-Agent": "Fontick/1.2.0"})
    with urlopen(request) as response:
        return response.read().decode("utf-8")


def data_lines(text: str):
    for raw_line in text.splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if line:
            yield line


def parse_code_point_range(value: str) -> tuple[int, int]:
    if ".." in value:
        start, end = value.split("..", 1)
        return int(start, 16), int(end, 16)
    code_point = int(value, 16)
    return code_point, code_point


def assign_range(values: list, start: int, end: int, value) -> None:
    values[start : end + 1] = [value] * (end - start + 1)


def parse_general_categories(text: str) -> list[str]:
    values = ["Cn"] * CODE_POINT_COUNT
    pending_range = None

    for line in data_lines(text):
        fields = line.split(";")
        code_point = int(fields[0], 16)
        name = fields[1]
        category = fields[2]

        if name.endswith(", First>"):
            if pending_range is not None:
                raise ValueError("nested UnicodeData First range")
            pending_range = (code_point, category)
        elif name.endswith(", Last>"):
            if pending_range is None or pending_range[1] != category:
                raise ValueError("unmatched UnicodeData Last range")
            assign_range(values, pending_range[0], code_point, category)
            pending_range = None
        else:
            values[code_point] = category

    if pending_range is not None:
        raise ValueError("unterminated UnicodeData First range")
    return values


def parse_scripts(text: str) -> list[str]:
    values = ["Unknown"] * CODE_POINT_COUNT
    for line in data_lines(text):
        code_points, script = (field.strip() for field in line.split(";", 1))
        start, end = parse_code_point_range(code_points)
        assign_range(values, start, end, script)
    return values


def parse_script_aliases(text: str) -> dict[str, str]:
    aliases = {}
    for line in data_lines(text):
        fields = [field.strip() for field in line.split(";")]
        if fields[0] == "sc":
            aliases[fields[1]] = fields[2]
    return aliases


def parse_script_extensions(
    text: str, scripts: list[str], aliases: dict[str, str]
) -> list[tuple[str, ...]]:
    values = [None] * CODE_POINT_COUNT
    for line in data_lines(text):
        code_points, short_names = (field.strip() for field in line.split(";", 1))
        names = tuple(sorted(aliases[name] for name in short_names.split()))
        start, end = parse_code_point_range(code_points)
        assign_range(values, start, end, names)

    return [value if value is not None else (scripts[index],) for index, value in enumerate(values)]


def compact_ranges(
    general_categories: list[str], script_extensions: list[tuple[str, ...]]
):
    start = 0
    previous = (general_categories[0], script_extensions[0])
    for code_point in range(1, CODE_POINT_COUNT):
        current = (general_categories[code_point], script_extensions[code_point])
        if current != previous:
            yield start, code_point - 1, previous[0], previous[1]
            start = code_point
            previous = current
    yield start, CODE_POINT_COUNT - 1, previous[0], previous[1]


def rust_string(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def render(
    ranges: list[tuple[int, int, str, tuple[str, ...]]],
    general_categories: list[str],
    script_names: list[str],
) -> str:
    lines = [
        f'pub const UNICODE_VERSION: &str = "{UNICODE_VERSION}";',
        "",
        "pub struct UnicodeRange {",
        "    pub from: u32,",
        "    pub to: u32,",
        "    pub general_category: &'static str,",
        "    pub script_extensions: &'static [&'static str],",
        "}",
        "",
        "#[rustfmt::skip]",
        "pub static UNICODE_RANGES: &[UnicodeRange] = &[",
    ]

    for start, end, category, scripts in ranges:
        script_values = ", ".join(rust_string(script) for script in scripts)
        lines.extend(
            (
                "    UnicodeRange {",
                f"        from: 0x{start:X},",
                f"        to: 0x{end:X},",
                f"        general_category: {rust_string(category)},",
                f"        script_extensions: &[{script_values}],",
                "    },",
            )
        )
    lines.extend(("];", ""))

    lines.append("#[rustfmt::skip]")
    lines.append("pub static GENERAL_CATEGORIES: &[&str] = &[")
    lines.extend(f"    {rust_string(category)}," for category in general_categories)
    lines.extend(("];", ""))

    lines.append("#[rustfmt::skip]")
    lines.append("pub static SCRIPT_NAMES: &[&str] = &[")
    lines.extend(f"    {rust_string(script)}," for script in script_names)
    lines.extend(("];", ""))
    return "\n".join(lines)


def main() -> None:
    sources = {filename: download(filename) for filename in FILES}
    general_category = parse_general_categories(sources["UnicodeData.txt"])
    script = parse_scripts(sources["Scripts.txt"])
    aliases = parse_script_aliases(sources["PropertyValueAliases.txt"])
    script_extensions = parse_script_extensions(
        sources["ScriptExtensions.txt"], script, aliases
    )

    ranges = list(compact_ranges(general_category, script_extensions))
    categories = sorted(set(general_category))
    scripts = sorted({name for names in script_extensions for name in names})
    output = render(ranges, categories, scripts)
    with OUTPUT_PATH.open("w", encoding="utf-8", newline="\n") as file:
        file.write(output)


if __name__ == "__main__":
    main()
