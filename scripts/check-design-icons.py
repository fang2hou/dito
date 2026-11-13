#!/usr/bin/env python3
"""Check shared icon geometry in a saved FigCraft file without changing it."""

import argparse
import json
import math
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IDENTITY = [1, 0, 0, 1, 0, 0]
SLOTS = {24: 14, 32: 18, 36: 20, 40: 22}
KEYLINE_MAX = 20
KEYLINE_CENTER = 1
KEYLINE_OFFSET_EXEMPT = {"lucide/check"}
TEXT_BUTTON_SLOTS = {
    "Regular": (36, 16, 14),
    "Large": (40, 18, 16),
    "Hero": (50, 20, 18),
}


def load_document(path):
    if path.suffix == ".json":
        return json.loads(path.read_text())
    with tempfile.TemporaryDirectory(prefix="dito-design-icons-") as directory:
        exported = Path(directory) / "design.json"
        subprocess.run(
            ["figcraft-cli", "convert", str(path), str(exported)],
            check=True,
            capture_output=True,
            text=True,
        )
        return json.loads(exported.read_text())


def check_document(document):
    nodes = {int(key): node for key, node in document["nodes"].items()}
    icons = {
        key: node
        for key, node in nodes.items()
        if node["type"] == "component" and node.get("name", "").startswith("lucide/")
    }
    errors = []
    extents = {}

    def number(actual, expected, label):
        if not math.isclose(actual, expected, abs_tol=0.04):
            errors.append(f"{label}: {actual:g}, expected {expected:g}")

    def compare(source, actual, scale, label):
        if source["type"] != actual["type"]:
            errors.append(f"{label}: contour type differs from its master")
            return
        for field in ("width", "height"):
            number(
                actual.get(field, 0), source.get(field, 0) * scale, f"{label}, {field}"
            )
        expected = source.get("transform", IDENTITY)
        current = actual.get("transform", IDENTITY)
        for index, value in enumerate(expected):
            factor = scale if index >= 4 else 1
            number(current[index], value * factor, f"{label}, transform[{index}]")
        if source.get("strokes"):
            number(
                actual.get("stroke", {}).get("weight", 0),
                source["stroke"]["weight"] * scale,
                f"{label}, stroke",
            )
        for index, value in enumerate(source.get("corners", {}).get("radii", [])):
            current = actual.get("corners", {}).get("radii", [0, 0, 0, 0])
            number(current[index], value * scale, f"{label}, corner[{index}]")
        network = source.get("network", {})
        current_network = actual.get("network", {})
        for kind in ("vertices", "segments"):
            expected_parts = network.get(kind, [])
            actual_parts = current_network.get(kind, [])
            if len(expected_parts) != len(actual_parts):
                errors.append(f"{label}: vector {kind} differ from its master")
                continue
            for index, (part, current) in enumerate(zip(expected_parts, actual_parts)):
                if kind == "vertices":
                    for field in ("x", "y", "radius"):
                        number(
                            current.get(field, 0),
                            part.get(field, 0) * scale,
                            f"{label}, vertex[{index}].{field}",
                        )
                else:
                    for field in ("ta", "tb"):
                        for axis in range(2):
                            number(
                                current.get(field, [0, 0])[axis],
                                part.get(field, [0, 0])[axis] * scale,
                                f"{label}, segment[{index}].{field}[{axis}]",
                            )
        expected_children = source.get("children", [])
        actual_children = actual.get("children", [])
        if len(expected_children) != len(actual_children):
            errors.append(f"{label}: contour structure differs from its master")
            return
        for index, (expected_id, actual_id) in enumerate(
            zip(expected_children, actual_children)
        ):
            compare(nodes[expected_id], nodes[actual_id], scale, f"{label}/{index}")

    def control_family(node):
        parent = nodes.get(node.get("parent"), {})
        current = nodes.get(parent.get("component"), parent)
        while current:
            if current.get("name") in ("Icon Button", "HUD Button", "Close Button"):
                return current["name"], parent
            current = nodes.get(current.get("parent"), {})
        return None, parent

    def source_constraints(node):
        if node.get("strokes"):
            number(
                node.get("stroke", {}).get("weight", 0),
                2,
                f"{node['name']}, master stroke",
            )
        for child_id in node.get("children", []):
            child = nodes[child_id]
            if child.get("constraints") != {"horizontal": "scale", "vertical": "scale"}:
                errors.append(
                    f"{node['name']} ({child_id}): contour must follow its canvas"
                )
            source_constraints(child)

    def source_bounds(node, matrix, label):
        local = node.get("transform", IDENTITY)
        a, b, c, d, x, y = matrix
        e, f, g, h, u, v = local
        matrix = [
            a * e + c * f,
            b * e + d * f,
            a * g + c * h,
            b * g + d * h,
            a * u + c * v + x,
            b * u + d * v + y,
        ]
        children = node.get("children", [])
        if children:
            for child_id in children:
                source_bounds(nodes[child_id], matrix, label)
            return
        a, b, c, d, x, y = matrix
        radius = (
            node.get("stroke", {}).get("weight", 0) / 2 if node.get("strokes") else 0
        )
        for px, py in [
            (0, 0),
            (node.get("width", 0), 0),
            (0, node.get("height", 0)),
            (node.get("width", 0), node.get("height", 0)),
        ]:
            cx, cy = a * px + c * py + x, b * px + d * py + y
            box = extents.setdefault(label, [24, 24, 0, 0])
            box[0], box[1] = min(box[0], cx - radius), min(box[1], cy - radius)
            box[2], box[3] = max(box[2], cx + radius), max(box[3], cy + radius)
            if min(cx, cy) - radius < 0.96 or max(cx, cy) + radius > 23.04:
                errors.append(
                    f"{label} ({node['id']}): contour exceeds its 1 pt canvas safe zone"
                )
                break

    for icon in icons.values():
        number(icon["width"], 24, f"{icon['name']}, master width")
        number(icon["height"], 24, f"{icon['name']}, master height")
        source_constraints(icon)
        for child_id in icon.get("children", []):
            source_bounds(nodes[child_id], IDENTITY, icon["name"])
        box = extents.get(icon["name"])
        if box:
            width, height = box[2] - box[0], box[3] - box[1]
            if max(width, height) > KEYLINE_MAX + 0.05:
                errors.append(
                    f"{icon['name']}: ink box {width:g} by {height:g} exceeds the {KEYLINE_MAX} pt keyline"
                )
            if icon["name"] not in KEYLINE_OFFSET_EXEMPT:
                for axis, low, high in ((0, box[0], box[2]), (1, box[1], box[3])):
                    if abs((low + high) / 2 - 12) > KEYLINE_CENTER + 0.05:
                        errors.append(
                            f"{icon['name']}: ink box center is {(low + high) / 2:g} on axis {axis}, beyond {KEYLINE_CENTER} pt of the canvas center"
                        )

    instances = 0
    for node in nodes.values():
        for name, value in node.get("props", {}).items():
            references = []
            if (
                name in ("Icon", "Trailing")
                and isinstance(value, int)
                and not isinstance(value, bool)
            ):
                references = [value]
            elif isinstance(value, dict) and value.get("type") == "instanceSwap":
                references = [value.get("default"), *value.get("preferred", [])]
            for reference in references:
                if reference is not None and nodes.get(reference, {}).get(
                    "type"
                ) not in (
                    "component",
                    "componentSet",
                ):
                    errors.append(
                        f"{node['name']} ({node['id']}), {name}: missing component {reference}"
                    )
        source = icons.get(node.get("component"))
        if node["type"] != "instance" or source is None:
            continue
        instances += 1
        label = f"{source['name']} ({node['id']})"
        family, parent = control_family(node)
        slot = node["width"]
        if family == "HUD Button":
            slot = 14 * float(parent.get("plugin_data", {}).get("hudMotionScale", 1))
        elif family in ("Icon Button", "Close Button"):
            slot = SLOTS.get(round(parent.get("width", 0)), slot)
        number(node["width"], slot, f"{label}, slot width")
        number(node["height"], slot, f"{label}, slot height")
        compare(
            {**source, "type": "instance", "transform": IDENTITY},
            {**node, "transform": IDENTITY, "width": slot, "height": slot},
            slot / source["width"],
            label,
        )
    check_hud_outcomes(document, nodes, icons, number, errors)
    check_content_and_menus(document, nodes, number, errors)
    check_application_identities(nodes, number, errors)
    check_control_icon_contracts(nodes, number, errors)
    return len(icons), instances, errors


def check_control_icon_contracts(nodes, number, errors):
    """Validate each control family instead of accepting any proportional icon."""
    mains = {}
    for node in nodes.values():
        if node["type"] != "component":
            continue
        ancestor = nodes.get(node.get("parent"), {})
        while ancestor and ancestor["type"] != "componentSet":
            ancestor = nodes.get(ancestor.get("parent"), {})
        mains[node["id"]] = (ancestor.get("name"), node)
    for node in nodes.values():
        family, source = mains.get(node.get("component", node["id"]), (None, {}))
        expected = {}
        if family == "Button":
            size = next(
                (key for key in TEXT_BUTTON_SLOTS if f"Size={key}" in source["name"]),
                None,
            )
            if size is None:
                errors.append(f"Button {node['id']}: missing size preset")
                continue
            height, leading, trailing = TEXT_BUTTON_SLOTS[size]
            number(node["height"], height, f"Button {node['id']}, preset height")
            children = node.get("children", [])
            if (
                len(children) > 1
                and not nodes[children[1]].get("visible", True)
                and math.isclose(node["width"], node["height"], abs_tol=0.04)
            ):
                errors.append(f"Button {node['id']}: icon-only actions use Icon Button")
            expected = {0: leading, 2: trailing}
        elif family == "Menu Item":
            app_menu = node.get("plugin_data", {}).get("applicationIdentityRole")
            expected = {0: 24 if app_menu == "menu" else 16, 2: 14}
        elif family == "Select":
            expected = {1: 14}
        elif family == "Context Help":
            for field in ("width", "height"):
                number(node[field], 24, f"Context help {node['id']}, target {field}")
            expected = {0: 16}
        elif family in ("Icon Button", "Close Button", "HUD Button"):
            diameter = node["width"]
            if family == "HUD Button":
                # Explicit motion poses and enlarged review samples retain one uniform slot ratio.
                motion_scale = float(node.get("plugin_data", {}).get("hudMotionScale", 1))
                if not math.isfinite(motion_scale) or motion_scale <= 0:
                    errors.append(f"HUD button {node['id']}: invalid motion scale")
                    continue
                number(diameter, 24 * motion_scale, f"HUD button {node['id']}, diameter")
                expected = {0: 14 * motion_scale}
            else:
                if family == "Close Button":
                    number(diameter, 32, f"Close button {node['id']}, diameter")
                if round(diameter) not in SLOTS:
                    errors.append(f"Icon button {node['id']}: unknown size preset")
                    continue
                expected = {0: SLOTS[round(diameter)]}
            number(node["height"], diameter, f"Icon button {node['id']}, square")
        for index, canvas in expected.items():
            children = node.get("children", [])
            if index >= len(children):
                errors.append(f"Control {node['id']}: missing icon slot {index}")
                continue
            icon = nodes[children[index]]
            label = f"Control {node['id']}, icon {index}"
            for field in ("width", "height"):
                number(icon[field], canvas, f"{label}, {field}")
            if icon.get("visible", True):
                transform = icon.get("transform", IDENTITY)
                top = min(
                    transform[5] + transform[1] * x + transform[3] * y
                    for x, y in (
                        (0, 0),
                        (icon["width"], 0),
                        (0, icon["height"]),
                        (icon["width"], icon["height"]),
                    )
                )
                number(
                    top,
                    (node["height"] - canvas) / 2,
                    f"{label}, vertical center",
                )


def check_application_identities(nodes, number, errors):
    """Keep native artwork and generic glyphs inside their declared UI slots."""
    selects = {
        node["id"]
        for node in nodes.values()
        if node["type"] == "component" and node.get("name") == "Application Select"
    }
    for node in nodes.values():
        menu = node.get("plugin_data", {}).get("applicationIdentityRole") == "menu"
        select = node["id"] in selects or node.get("component") in selects
        if not menu and not select:
            continue
        if not node.get("children"):
            errors.append(f"Application identity {node['id']}: missing content")
            continue
        identity = nodes[node["children"][0]]
        if not identity.get("visible", True):
            continue
        slot = 24 if menu else 20
        label = f"Application identity {node['id']}"
        for field in ("width", "height"):
            number(identity[field], slot, f"{label}, {field}")
        source = nodes.get(identity.get("component"), {})
        name = source.get("name", "")
        if name.startswith("lucide/"):
            errors.append(f"{label}: use an application identity template")
        if name not in ("application/other", "application/add"):
            continue
        glyph = nodes[identity["children"][0]]
        canvas = 20 if menu else 18
        for field in ("width", "height"):
            number(glyph[field], canvas, f"{label}, generic glyph {field}")
        transform = glyph.get("transform", IDENTITY)
        for axis in (4, 5):
            number(transform[axis], (slot - canvas) / 2, f"{label}, glyph center")


def check_content_and_menus(document, nodes, number, errors):
    natural_styles = {
        "Product/JA/Body",
        "Product/JA/Label",
        "Product/JA/Caption",
        "Product/JA/Select",
        "Product/JA/Menu",
    }
    natural_ids = set()
    for style in document["styles"]:
        if style.get("name") in natural_styles:
            natural_ids.add(style["id"])
            number(
                style["style"]["letter_spacing"]["value"],
                0,
                f"{style['name']}, tracking",
            )
    separator_ids = {
        node["id"]
        for node in nodes.values()
        if node["type"] == "component" and node.get("name") == "Menu Separator"
    }
    item_ids = {
        node["id"]
        for node in nodes.values()
        if node["type"] == "component"
        and nodes.get(node.get("parent"), {}).get("name") == "Menu Item"
    }
    for node in nodes.values():
        role = node.get("plugin_data", {}).get("typographyRole")
        if node["type"] == "text" and (
            role in ("content", "name", "value", "natural-control")
            or node.get("styles", {}).get("text") in natural_ids
        ):
            number(
                node.get("style", {}).get("letter_spacing", {}).get("value", 0),
                0,
                f"Text {node['id']}, natural tracking",
            )
            for run in node.get("runs", []):
                number(
                    run.get("style", {}).get("letter_spacing", {}).get("value", 0),
                    0,
                    f"Text {node['id']}, inline tracking",
                )
        if node.get("component") in item_ids or node["id"] in item_ids:
            number(node["height"], 34, f"Menu item {node['id']}, fixed row height")
        if (
            node.get("component") not in separator_ids
            and node["id"] not in separator_ids
        ):
            continue
        label = f"Menu separator {node['id']}"
        number(node["height"], 9, label + ", occupied height")
        if node.get("fills"):
            errors.append(label + ": only the inner rule paints, not its spacing row")
        layout = node.get("layout", {})
        if layout.get("padding") != [4, 0, 4, 0]:
            errors.append(label + ": reserve 4 pt above and below the rule")
        children = node.get("children", [])
        if len(children) != 1:
            errors.append(label + ": expected one rule inside its spacing row")
            continue
        rule = nodes[children[0]]
        number(rule["height"], 1, label + ", rule thickness")
        number(rule["width"], node["width"], label + ", rule width")
        number(rule.get("transform", IDENTITY)[5], 4, label + ", rule inset")
        if node["type"] != "instance":
            continue
        parent = nodes[node["parent"]]
        number(parent.get("layout", {}).get("gap", 0), 2, label + ", item gap")
        siblings = parent["children"]
        index = siblings.index(node["id"])
        y = node.get("transform", IDENTITY)[5]
        if index:
            previous = nodes[siblings[index - 1]]
            gap = y - previous.get("transform", IDENTITY)[5] - previous["height"]
            number(gap, 2, label + ", preceding row gap")
        if index + 1 < len(siblings):
            following = nodes[siblings[index + 1]]
            gap = following.get("transform", IDENTITY)[5] - y - node["height"]
            number(gap, 2, label + ", following row gap")


def check_hud_outcomes(document, nodes, icons, number, errors):
    """Check explicit outcome roles and fixed animation coordinates, not color taste."""
    variables = {v["name"]: v["id"] for v in document["variables"]["variables"]}

    def capsule(node):
        while node:
            name = node.get("name", "")
            if (
                node.get("plugin_data", {}).get("hudOutcome")
                or name.startswith(("hud-capsule", "copied-storyboard"))
                or name == "outcome-storyboard"
            ):
                return node
            node = nodes.get(node.get("parent"))
        return None

    def role(node, field, name):
        expected = variables.get(name)
        if expected is None or node.get("bound", {}).get(field) != expected:
            errors.append(f"HUD {node['id']}: {field} must bind to {name}")

    def foreground(node, name):
        if node.get("strokes"):
            role(node, "stroke", name)
        if node.get("fills"):
            role(node, "fill", name)
        for child in node.get("children", []):
            foreground(nodes[child], name)

    def center(node, cap):
        x, y, current = 0, 0, node
        while current["id"] != cap["id"]:
            matrix = current.get("transform", IDENTITY)
            x += matrix[4]
            y += matrix[5]
            current = nodes[current["parent"]]
        number(
            y + node["height"] / 2,
            cap["height"] / 2,
            f"HUD {node['id']}, vertical center",
        )
        return x, y

    check_master = next(icon for icon in icons.values() if icon["name"] == "lucide/check")
    check_glyph = nodes[check_master["children"][0]]
    check_origin = check_glyph.get("transform", IDENTITY)
    check_points = sorted(
        (
            (vertex["x"] + check_origin[4]) * 18 / 24,
            (vertex["y"] + check_origin[5]) * 18 / 24,
        )
        for vertex in check_glyph["network"]["vertices"]
    )

    for node in nodes.values():
        cap = capsule(node)
        if cap is None:
            continue
        data = node.get("plugin_data", {})
        if data.get("hudOutcome"):
            role(node, "fill", "hud/capsule_" + data["hudOutcome"])
        source = icons.get(node.get("component"), {})
        parent = nodes.get(node.get("parent"), {})
        outcome = data.get("hudOutcomeIcon")
        if parent.get("name") == "hud-mid":
            outcome = {"lucide/check": "inserted", "lucide/copy": "copied"}.get(
                source.get("name"), outcome
            )
        if outcome:
            canvas = 18
            number(node["width"], canvas, f"HUD {node['id']}, outcome width")
            number(node["height"], canvas, f"HUD {node['id']}, outcome height")
            foreground(node, "hud/on_" + outcome)
            center(node, cap)
        label = data.get("hudCopiedLabel")
        if node["type"] == "text" and (
            label
            or node.get("name") == "t-copied"
            or node.get("name", "").startswith("copied-glyph-")
        ):
            role(node, "fill", "hud/on_copied")
            number(node["height"], 18, f"HUD {node['id']}, copied line box")
            line_height = node.get("style", {}).get("line_height", {})
            if line_height.get("unit") != "pixels":
                errors.append(f"HUD {node['id']}: copied line height must be 18 pt")
            else:
                number(
                    line_height.get("value", 0),
                    18,
                    f"HUD {node['id']}, copied line height",
                )
            if label == "settled":
                center(node, cap)
        if "hudCheckProgress" not in data:
            continue
        number(node["width"], 18, f"HUD {node['id']}, reveal canvas width")
        number(node["height"], 18, f"HUD {node['id']}, reveal canvas height")
        x, _ = center(node, cap)
        number(
            x + node["width"] / 2,
            cap["width"] / 2,
            f"HUD {node['id']}, reveal canvas center",
        )
        progress = float(data["hudCheckProgress"])
        fixed_reveal = data.get("hudCheckFixedReveal")
        if not fixed_reveal and (progress <= 0 or progress >= 1):
            continue
        a, b, c = check_points
        ab, bc = math.dist(a, b), math.dist(b, c)
        length = progress * (ab + bc)
        if length <= ab:
            tip = tuple(a[i] + (b[i] - a[i]) * length / ab for i in range(2))
            # Continuous-path storyboards retain three vertices across the turn.
            expected = [a, tip, tip] if data.get("hudCheckContinuous") else [a, tip]
        else:
            expected = [
                a,
                b,
                tuple(b[i] + (c[i] - b[i]) * (length - ab) / bc for i in range(2)),
            ]
        trace = nodes[node["children"][0]]
        if fixed_reveal:
            reveal = trace
            tip = expected[-1]
            expected = [a, b, c]
            number(reveal["height"], 18, f"HUD {node['id']}, fixed reveal height")
            number(
                reveal["width"], 18 if progress >= 1 else tip[0],
                f"HUD {node['id']}, reveal edge",
            )
            for axis, value in enumerate(reveal.get("transform", IDENTITY)):
                number(value, IDENTITY[axis], f"HUD {node['id']}, reveal origin {axis}")
            if not reveal.get("clip"):
                errors.append(f"HUD {node['id']}: reveal must clip the fixed contour")
            pen = nodes[node["children"][1]]
            pen_matrix = pen.get("transform", IDENTITY)
            for axis in range(2):
                number(pen_matrix[axis + 4] + 0.75, tip[axis],
                       f"HUD {node['id']}, pen tip {axis}")
            trace = nodes[reveal["children"][0]]
        matrix = trace.get("transform", IDENTITY)
        actual = [
            (vertex["x"] + matrix[4], vertex["y"] + matrix[5])
            for vertex in trace.get("network", {}).get("vertices", [])
        ]
        if len(actual) != len(expected):
            errors.append(
                f"HUD {node['id']}: reveal must follow the canonical check path"
            )
        else:
            for index, (point, target) in enumerate(zip(actual, expected)):
                for axis in range(2):
                    number(
                        point[axis],
                        target[axis],
                        f"HUD {node['id']}, reveal point {index}/{axis}",
                    )
        number(
            trace.get("stroke", {}).get("weight", 0),
            1.5,
            f"HUD {node['id']}, reveal stroke",
        )
        foreground(trace, "hud/on_inserted")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "file", nargs="?", type=Path, default=ROOT / "design/dito.figcraft"
    )
    args = parser.parse_args()
    masters, instances, errors = check_document(load_document(args.file.resolve()))
    print(f"Checked {masters} shared masters and {instances} icon instances.")
    if errors:
        for error in errors[:40]:
            print(f"  {error}")
        if len(errors) > 40:
            print(f"  ... {len(errors) - 40} more geometry errors")
        print(f"FAIL: {len(errors)} errors")
        raise SystemExit(1)
    print(
        "PASS: icon geometry, control sizes and centers, HUD outcomes, "
        "content tracking and menu spacing."
    )


if __name__ == "__main__":
    main()
