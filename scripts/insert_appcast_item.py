#!/usr/bin/env python3
"""appcast.xmlの<channel>に新しい<item>を挿入する（新しい順に先頭へ）。"""

import argparse
import xml.etree.ElementTree as ET

SPARKLE_NS = "http://www.andymatuschak.org/xml-namespaces/sparkle"
ET.register_namespace("sparkle", SPARKLE_NS)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("appcast_path")
    parser.add_argument("--version", required=True)
    parser.add_argument("--download-url", required=True)
    parser.add_argument("--pub-date", required=True)
    parser.add_argument("--ed-signature", required=True)
    parser.add_argument("--length", required=True)
    parser.add_argument("--min-os", required=True)
    args = parser.parse_args()

    tree = ET.parse(args.appcast_path)
    channel = tree.getroot().find("channel")
    if channel is None:
        raise SystemExit("appcast.xmlに<channel>要素がありません")

    item = ET.Element("item")

    title = ET.SubElement(item, "title")
    title.text = f"v{args.version}"

    pub_date = ET.SubElement(item, "pubDate")
    pub_date.text = args.pub_date

    sparkle_version = ET.SubElement(item, f"{{{SPARKLE_NS}}}version")
    sparkle_version.text = args.version

    short_version = ET.SubElement(item, f"{{{SPARKLE_NS}}}shortVersionString")
    short_version.text = args.version

    min_system = ET.SubElement(item, f"{{{SPARKLE_NS}}}minimumSystemVersion")
    min_system.text = args.min_os

    enclosure = ET.SubElement(item, "enclosure")
    enclosure.set("url", args.download_url)
    enclosure.set(f"{{{SPARKLE_NS}}}edSignature", args.ed_signature)
    enclosure.set("length", args.length)
    enclosure.set("type", "application/octet-stream")

    # 新しいリリースを先頭（既存itemより前）に挿入する
    existing_items = channel.findall("item")
    if existing_items:
        insert_index = list(channel).index(existing_items[0])
        channel.insert(insert_index, item)
    else:
        channel.append(item)

    ET.indent(tree, space="    ")
    tree.write(args.appcast_path, encoding="utf-8", xml_declaration=True)

    # ET.write appends a trailing newline is not guaranteed; make sure the file
    # ends cleanly.
    with open(args.appcast_path, "a", encoding="utf-8") as f:
        f.write("\n")


if __name__ == "__main__":
    main()
