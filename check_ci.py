import urllib.request, json, subprocess
token = subprocess.check_output(['gh', 'auth', 'token']).decode().strip()
req = urllib.request.Request('https://api.github.com/repos/cueikusuta/agricola-re-ai/commits/308ce83/check-runs', headers={'Authorization': f'Bearer {token}', 'Accept': 'application/vnd.github.v3+json'})
with urllib.request.urlopen(req) as response:
    data = json.load(response)
    for run in data.get('check_runs', []):
        print(f"{run['name']}: {run['status']} {run.get('conclusion', 'pending')}")