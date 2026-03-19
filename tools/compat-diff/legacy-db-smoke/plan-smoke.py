#!/usr/bin/env python3

from __future__ import annotations

import os
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore[no-redef]


def load_config(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def require_env(env_name: str) -> str:
    value = os.environ.get(env_name)
    if not value:
        raise SystemExit(f"missing required env var: {env_name}")
    return value


def main() -> int:
    config_path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).with_name("cases.toml")
    config = load_config(config_path)

    env_config = config.get("env", {})
    resolved_env = {key: require_env(value) for key, value in env_config.items()}

    print(config.get("schema", config_path.stem))
    print(f"output_dir: {config.get('output_dir', '')}")
    print(f"result_archive_dir: {config.get('result_archive_dir', '')}")
    print(f"java_base_url: {resolved_env.get('java_base_url', '')}")
    print(f"rust_base_url: {resolved_env.get('rust_base_url', '')}")
    print(f"java_config_dir: {resolved_env.get('java_config_dir', '')}")
    print(f"rust_config_dir: {resolved_env.get('rust_config_dir', '')}")
    print("planned_cases:")

    for case in config.get("cases", []):
        setup = case.get("setup", [])
        setup_names = ", ".join(step.get("name", "") for step in setup) if setup else ""
        suffix = f" [setup: {setup_names}]" if setup_names else ""
        print(f"- {case['id']}: {case['method']} {case['path']}{suffix}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
