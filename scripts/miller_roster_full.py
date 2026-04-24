#!/usr/bin/env python3
"""Miller County Sheriff Roster - Full scraper with pagination support."""
import sys, re, json, subprocess, urllib.request

ACCOUNT = "crustaison@gmail.com"
GOG_PASSWORD = "crustaison"
BASE_URL = "https://www.millercountysheriff.org"

def fetch_page(url):
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(req, timeout=20) as r:
        return r.read().decode("utf-8", errors="replace")

def parse_roster_page(html):
    inmates = []
    blocks = re.split(r'<strong class="ptitles"', html)
    for block in blocks[1:]:
        name_m = re.match(r'[^>]*>([^<]+)</strong>', block)
        if not name_m: continue
        name = name_m.group(1).strip()
        booking_m = re.search(r'Booking #:.*?<span class="text2">(\d+)</span>', block, re.DOTALL)
        booking = booking_m.group(1).strip() if booking_m else ""
        if booking: inmates.append((name, booking))
    return inmates

def get_all_inmates(released=False):
    all_inmates = []
    grp = 0
    while True:
        if grp == 0:
            url = f"{BASE_URL}/roster.php?{'released=1' if released else 'released=0'}"
        else:
            url = f"{BASE_URL}/roster.php?{'released=1' if released else 'released=0'}&grp={grp}"
        print(f"  {'Released' if released else 'Current'} page {grp//40 + 1}...")
        html = fetch_page(url)
        inmates = parse_roster_page(html)
        if not inmates:
            break
        all_inmates.extend(inmates)
        print(f"    Found {len(inmates)} (total: {len(all_inmates)})")
        grp += 40
    return all_inmates

def parse_profile(html, booking_num):
    data = {"booking_num": booking_num}
    pattern = r'<span class="tbold">([^<]+):?</span></div>\s*<div class="cell inmate_profile_data_content">([^<]*)</div>'
    matches = re.findall(pattern, html, re.DOTALL)
    
    for label, value in matches:
        label = label.strip()
        value = value.strip()
        label_key = label.rstrip(':').strip()
        
        if label_key == "Age":
            data["age"] = value
        elif label_key == "Gender":
            data["gender"] = value
        elif label_key == "Race":
            data["race"] = value
        elif label_key == "Arresting Agency":
            data["arresting_agency"] = value
        elif label_key == "Booking Date":
            data["booking_date"] = value
        elif label_key == "Bond":
            bond_val = re.sub(r'[^\d,]', '', value)
            data["bond"] = f"${bond_val}" if bond_val else ""
    
    charges_pattern = r'Charges:</span></div>\s*<div class="cell inmate_profile_data_content"[^>]*>(.*?)</div>'
    charges_match = re.search(charges_pattern, html, re.DOTALL)
    if charges_match:
        charges = charges_match.group(1)
        charges = re.sub(r'<[^>]+>', ' ', charges)
        charges = re.sub(r'\s+', ' ', charges).strip()
        data["charges"] = charges
    else:
        data["charges"] = ""
    
    return data

def main():
    spreadsheet_id = sys.argv[1] if len(sys.argv) > 1 else "1nHIu4gzA3aLHV3BH1zAGEFAM5MGf9t9DubgzMoQ4GPU"
    
    print("Fetching current inmates...")
    current = get_all_inmates(released=False)
    
    print("Fetching released inmates...")
    released = get_all_inmates(released=True)
    
    # Dedupe
    all_roster = current.copy()
    existing = {b for n,b in current}
    for n,b in released:
        if b not in existing:
            all_roster.append((n,b))
    print(f"Total unique: {len(all_roster)}")
    
    all_data = []
    for i, (name, booking) in enumerate(all_roster):
        print(f"  [{i+1}/{len(all_roster)}] Profile {booking} ({name})...")
        try:
            profile_html = fetch_page(f"{BASE_URL}/roster_view.php?booking_num={booking}")
            profile_data = parse_profile(profile_html, booking)
            profile_data["name"] = name
            all_data.append(profile_data)
        except Exception as e:
            print(f"    ERROR: {e}")
            all_data.append({"name": name, "booking_num": booking})
    
    headers = ["Name", "Booking #", "Age", "Gender", "Race", "Arresting Agency", "Booking Date", "Charges", "Bond"]
    rows = [headers]
    for d in all_data:
        rows.append([d.get("name",""), d.get("booking_num",""), d.get("age",""), d.get("gender",""), d.get("race",""), d.get("arresting_agency",""), d.get("booking_date",""), d.get("charges",""), d.get("bond","")])
    
    values_json = json.dumps(rows)
    result = subprocess.run(["/home/sean/.npm-global/bin/gog", "sheets", "update", spreadsheet_id, "Sheet1!A1", "-a", ACCOUNT, f"--values-json={values_json}"], capture_output=True, text=True, env={"GOG_KEYRING_PASSWORD": GOG_PASSWORD, "HOME": "/home/sean"})
    
    if result.returncode == 0:
        print(f"Success! {len(all_data)} inmates written.")
        print(f"Sheet: https://docs.google.com/spreadsheets/d/{spreadsheet_id}")
    else:
        print(f"Error: {result.stderr}")

if __name__ == "__main__": main()
