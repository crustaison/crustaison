#!/usr/bin/env python3
"""
Unified Missouri Sheriff Roster scraper.
Supports Miller, Camden, and Morgan counties — all use the same CMS.

Usage:
    python3 sheriff_roster.py [county]  [mode]
    python3 sheriff_roster.py miller    all
    python3 sheriff_roster.py all       all   (default)
"""
import sys
import re
import json
import subprocess
import urllib.request
import time

COUNTIES = {
    "miller": {
        "base_url": "https://www.millercountysheriff.org",
        "spreadsheet_id": "1iMowgi3paBJprv00H1s46v78sBoKzpDGu4sZEaizwFE",
    },
    "camden": {
        "base_url": "https://www.camdencountymosheriff.org",
        "spreadsheet_id": "1nxyeVvyOTUXn-zptlwg6mbB9lcdVgtSM8ur6gJVmoo4",
    },
    "morgan": {
        "base_url": "https://www.morgancountymoso.org",
        "spreadsheet_id": "1PPgoJ0RgWbr7b2L98qlfWf9SP5wYaol60QFyxA679gU",
    },
}

ACCOUNT = "crustaison@gmail.com"
GOG_BIN = "/home/sean/.npm-global/bin/gog"
GOG_ENV = {"GOG_KEYRING_PASSWORD": "crustaison", "HOME": "/home/sean", "PATH": "/usr/bin:/bin"}

HEADER = ["Name", "Booking #", "Status", "Age", "Gender", "Race",
          "Arresting Agency", "Booking Date", "Release Date", "Charges", "Bond"]


def fetch(url, retries=3):
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
            with urllib.request.urlopen(req, timeout=15) as r:
                return r.read().decode("utf-8", errors="replace")
        except Exception as e:
            if attempt == retries - 1:
                raise
            time.sleep(2)


def parse_roster_page(html):
    inmates = []
    for block in re.split(r'<strong class="ptitles"', html)[1:]:
        name_m = re.match(r'[^>]*>([^<]+)</strong>', block)
        booking_m = re.search(r'booking_num=(\w+)', block)
        if name_m and booking_m:
            inmates.append((name_m.group(1).strip(), booking_m.group(1).strip()))
    return inmates


def fetch_all_inmates(base_url, mode="all"):
    """Fetches all inmates by following grp= pagination links from each page's HTML."""
    all_inmates = []
    seen_bookings = set()

    sources = []
    if mode in ("current", "all"):
        sources.append((f"{base_url}/roster.php", "current"))
    if mode in ("released", "all"):
        sources.append((f"{base_url}/roster.php?released=1", "released"))

    for base_roster_url, label in sources:
        visited_urls = set()
        queue = [base_roster_url]

        while queue:
            url = queue.pop(0)
            if url in visited_urls:
                continue
            visited_urls.add(url)

            try:
                html = fetch(url)
            except Exception as e:
                print(f"  Warning: failed to fetch {url}: {e}")
                continue

            for n, b in parse_roster_page(html):
                if b not in seen_bookings:
                    seen_bookings.add(b)
                    all_inmates.append((n, b, label))

            sep = "&" if "?" in base_roster_url else "?"
            for g in re.findall(r'\bgrp=(\d+)', html):
                next_url = f"{base_roster_url}{sep}grp={g}"
                if next_url not in visited_urls:
                    queue.append(next_url)

            time.sleep(0.3)

    return all_inmates


def fetch_profile(base_url, booking_num):
    html = fetch(f"{base_url}/roster_view.php?booking_num={booking_num}")

    def pf(label):
        """Extract field value after a label — handles plain text and span-wrapped values."""
        m = re.search(
            rf'(?:tbold\b|inmate_profile_data_bold)[^>]*>\s*{re.escape(label)}\s*</[^>]+>',
            html, re.IGNORECASE
        )
        if not m:
            return ""
        rest = html[m.end():]
        # Try inmate_profile_data_content div first
        div_m = re.search(
            r'<div[^>]*inmate_profile_data_content[^>]*>(.*?)</div>',
            rest, re.DOTALL | re.IGNORECASE
        )
        if div_m:
            content = re.sub(r'<[^>]+>', '', div_m.group(1))
            content = content.replace('&nbsp;', '').strip()
            if content:
                return content
        # Fallback: any next div
        div_m2 = re.search(r'</div>\s*<div[^>]*>(.*?)</div>', rest, re.DOTALL | re.IGNORECASE)
        if div_m2:
            content = re.sub(r'<[^>]+>', '', div_m2.group(1))
            content = content.replace('&nbsp;', '').strip()
            return content
        return ""

    # Charges: anchor search to AFTER the Charges: label to avoid grabbing Release Date
    charges = ""
    charges_label_m = re.search(
        r'(?:tbold\b|inmate_profile_data_bold)[^>]*>\s*Charges:\s*</[^>]+>',
        html, re.IGNORECASE
    )
    if charges_label_m:
        charges_html = html[charges_label_m.end():]
        cm = re.search(r'class="text2">(.*?)</span>', charges_html, re.DOTALL | re.IGNORECASE)
        if cm:
            raw = cm.group(1)
            charges = re.sub(r'<br\s*/?>', ' | ', raw)
            charges = re.sub(r'<[^>]+>', '', charges).strip(' |').strip()
        else:
            # Plain div fallback
            dm = re.search(r'<div[^>]*inmate_profile_data_content[^>]*>(.*?)</div>', charges_html, re.DOTALL | re.IGNORECASE)
            if dm:
                charges = re.sub(r'<[^>]+>', '', dm.group(1)).replace('&nbsp;', '').strip()

    return [
        pf("Age:"),
        pf("Gender:"),
        pf("Race:"),
        pf("Arresting Agency:"),
        pf("Booking Date:"),
        pf("Release Date:"),
        charges,
        pf("Bond:"),
    ]


def get_sheet_state(spreadsheet_id):
    result = subprocess.run(
        [GOG_BIN, "sheets", "get", spreadsheet_id, "Sheet1!B:B", "-a", ACCOUNT],
        capture_output=True, text=True, env=GOG_ENV,
    )
    if result.returncode != 0 or not result.stdout.strip():
        return set(), 1
    lines = [l.strip() for l in result.stdout.strip().splitlines()]
    existing = set(l for l in lines[1:] if l)
    next_row = len(lines) + 1
    return existing, next_row


def sheets_clear(spreadsheet_id):
    subprocess.run(
        [GOG_BIN, "sheets", "clear", spreadsheet_id, "Sheet1!A:K", "-a", ACCOUNT],
        capture_output=True, text=True, env=GOG_ENV,
    )


def sheets_write(spreadsheet_id, range_name, rows):
    result = subprocess.run(
        [GOG_BIN, "sheets", "update", spreadsheet_id, range_name,
         "-a", ACCOUNT, f"--values-json={json.dumps(rows)}"],
        capture_output=True, text=True, env=GOG_ENV,
    )
    if result.returncode != 0:
        print(f"  Sheets error: {result.stderr or result.stdout}")
        return False
    return True


def run_county(county_name, mode="all", fresh=False):
    cfg = COUNTIES[county_name]
    base_url = cfg["base_url"]
    spreadsheet_id = cfg["spreadsheet_id"]

    print(f"\n{'='*60}")
    print(f"County: {county_name.upper()}  |  Mode: {mode}{'  |  FRESH' if fresh else ''}")
    print(f"{'='*60}")

    if fresh:
        print("Clearing sheet...")
        sheets_clear(spreadsheet_id)

    print("Fetching roster pages...")
    all_inmates = fetch_all_inmates(base_url, mode)
    print(f"Found {len(all_inmates)} inmates on site.")

    existing_nums, next_row = get_sheet_state(spreadsheet_id)
    print(f"Sheet has {len(existing_nums)} existing records (next row: {next_row}).")

    new_inmates = [(n, b, s) for n, b, s in all_inmates if b not in existing_nums]
    if not new_inmates:
        print("No new inmates to add.")
        return 0

    print(f"{len(new_inmates)} new inmates to add. Fetching profiles...")
    rows = []
    for i, (name, booking_num, status) in enumerate(new_inmates):
        try:
            profile = fetch_profile(base_url, booking_num)
            rows.append([name, booking_num, status] + profile)
            print(f"  [{i+1}/{len(new_inmates)}] {name} ({booking_num}) [{status}]")
        except Exception as e:
            print(f"  [{i+1}/{len(new_inmates)}] ERROR {booking_num}: {e}")
            rows.append([name, booking_num, status, "", "", "", "", "", "", "", ""])
        time.sleep(0.2)

    if next_row == 1:
        sheets_write(spreadsheet_id, "Sheet1!A1", [HEADER] + rows)
        print(f"Done. Wrote header + {len(rows)} inmates.")
    else:
        sheets_write(spreadsheet_id, f"Sheet1!A{next_row}", rows)
        print(f"Done. Appended {len(rows)} new inmates.")

    return len(rows)


def main():
    county_arg = sys.argv[1].lower() if len(sys.argv) > 1 else "all"
    mode_arg   = sys.argv[2].lower() if len(sys.argv) > 2 else "all"
    fresh      = "--fresh" in sys.argv

    if mode_arg not in ("current", "released", "all"):
        print(f"Unknown mode '{mode_arg}'. Use: current, released, all")
        sys.exit(1)

    if county_arg == "all":
        targets = list(COUNTIES.keys())
    elif county_arg in COUNTIES:
        targets = [county_arg]
    else:
        print(f"Unknown county '{county_arg}'. Use: miller, camden, morgan, all")
        sys.exit(1)

    total_new = 0
    for county in targets:
        try:
            total_new += run_county(county, mode_arg, fresh=fresh)
        except Exception as e:
            print(f"ERROR running {county}: {e}")

    print(f"\nAll done. {total_new} total new records added.")


if __name__ == "__main__":
    main()
