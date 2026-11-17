import urllib.request, json, subprocess
token = subprocess.check_output(['gh', 'auth', 'token']).decode().strip()
req = urllib.request.Request('https://api.github.com/repos/cueikusuta/agricola-re-ai/pulls/327', headers={'Authorization': f'Bearer {token}', 'Accept': 'application/vnd.github.v3+json'})
with urllib.request.urlopen(req) as response:
    data = json.load(response)
    body = data.get('body', '')
    print(body[:2000] if body else '(empty)')