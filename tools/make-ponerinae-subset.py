#!/usr/bin/env python3
"""Build a deterministic, portable Ponerinae acceptance fixture.

This is a fixture-maintenance tool. Runtime validation uses only the generated
Newick and TSV files and does not depend on Python or ETE.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tree", type=Path, required=True)
    parser.add_argument("--ranges", type=Path, required=True)
    parser.add_argument("--taxon-map", type=Path, required=True)
    parser.add_argument("--area-map", type=Path, required=True)
    parser.add_argument("--ete3-vendor", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--tips", type=int, default=32)
    return parser.parse_args()


def read_two_column_tsv(path: Path, left: str, right: str) -> dict[str, str]:
    with path.open("r", encoding="utf-8-sig", newline="") as handle:
        rows = csv.DictReader(handle, delimiter="\t")
        if rows.fieldnames != [left, right]:
            raise ValueError(f"{path} must have columns {left!r}, {right!r}")
        return {row[left]: row[right] for row in rows}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_ranges(
    path: Path,
    taxon_map: dict[str, str],
    area_map: dict[str, str],
) -> tuple[list[str], dict[str, tuple[int, ...]]]:
    with path.open("r", encoding="utf-8-sig", newline="") as handle:
        rows = csv.DictReader(handle)
        if rows.fieldnames is None or "Name" not in rows.fieldnames:
            raise ValueError(f"{path} must contain a Name column")
        source_areas = [name for name in rows.fieldnames if name not in {"ID", "Name"}]
        if set(source_areas) != set(area_map):
            raise ValueError("area map does not exactly cover the source range columns")
        areas = [area_map[name] for name in source_areas]
        ranges: dict[str, tuple[int, ...]] = {}
        for row in rows:
            source_taxon = row["Name"]
            taxon = taxon_map.get(source_taxon, source_taxon)
            values = tuple(int(row[name]) for name in source_areas)
            if any(value not in (0, 1) for value in values):
                raise ValueError(f"non-binary range value for {source_taxon}")
            if taxon in ranges:
                raise ValueError(f"duplicate mapped taxon {taxon}")
            ranges[taxon] = values
    return areas, ranges


def select_tips(tree, ranges: dict[str, tuple[int, ...]], count: int) -> list[str]:
    leaves = tree.get_leaves()
    by_name = {leaf.name: leaf for leaf in leaves}
    missing = sorted(set(by_name) - set(ranges))
    if missing:
        raise ValueError(f"range table is missing tree tips: {missing[:5]}")
    if count < len(next(iter(ranges.values()))) or count > len(leaves):
        raise ValueError("tip count must cover every area and not exceed the source tree")

    selected: list[str] = []

    def add_farthest(candidates: list[str]) -> None:
        remaining = [name for name in candidates if name not in selected]
        if not remaining:
            raise ValueError("cannot satisfy deterministic selection constraints")
        if not selected:
            choice = max(
                remaining,
                key=lambda name: (tree.get_distance(by_name[name]), name),
            )
        else:
            choice = max(
                remaining,
                key=lambda name: (
                    min(tree.get_distance(by_name[name], by_name[other]) for other in selected),
                    name,
                ),
            )
        selected.append(choice)

    area_count = len(next(iter(ranges.values())))
    for area_index in range(area_count):
        add_farthest([name for name in by_name if ranges[name][area_index] == 1])

    widespread = [name for name in by_name if sum(ranges[name]) > 1]
    if widespread:
        add_farthest(widespread)

    while len(selected) < count:
        add_farthest(list(by_name))

    leaf_order = {leaf.name: index for index, leaf in enumerate(leaves)}
    return sorted(selected, key=leaf_order.__getitem__)


def main() -> int:
    args = parse_args()
    if not args.ete3_vendor.is_dir():
        raise FileNotFoundError(f"ETE vendor directory not found: {args.ete3_vendor}")
    sys.path.insert(0, str(args.ete3_vendor.resolve()))
    from ete3 import Tree  # pylint: disable=import-outside-toplevel

    taxon_map = read_two_column_tsv(args.taxon_map, "source_taxon", "target_taxon")
    area_map = read_two_column_tsv(args.area_map, "source_area", "target_area")
    areas, ranges = load_ranges(args.ranges, taxon_map, area_map)

    tree = Tree(str(args.tree), format=1)
    selected = select_tips(tree, ranges, args.tips)
    selected_tree = tree.copy(method="deepcopy")
    selected_tree.prune(selected, preserve_branch_length=True)

    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=False)
    (output_dir / "tree.nwk").write_text(
        selected_tree.write(
            format=1,
            format_root_node=False,
            dist_formatter="%.17g",
        ).strip()
        + "\n",
        encoding="utf-8",
        newline="\n",
    )
    with (output_dir / "ranges.tsv").open("w", encoding="utf-8", newline="\n") as handle:
        writer = csv.writer(handle, delimiter="\t", lineterminator="\n")
        writer.writerow(["tip", *areas])
        for name in selected:
            writer.writerow([name, *ranges[name]])

    provenance = {
        "format": "biogeo-derived-fixture-provenance-v1",
        "source_tree_sha256": sha256(args.tree),
        "source_ranges_sha256": sha256(args.ranges),
        "source_taxon_map_sha256": sha256(args.taxon_map),
        "source_area_map_sha256": sha256(args.area_map),
        "selection": "one farthest representative per area, one farthest widespread tip, then farthest-point traversal; source leaf order for output",
        "tips": len(selected),
        "areas": areas,
        "selected_taxa": selected,
    }
    (output_dir / "provenance.json").write_text(
        json.dumps(provenance, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
