# Flask

Application factory Flask starter.

## Getting started

```bash
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
flask --app wsgi run --debug
# or
python wsgi.py
```

Open http://127.0.0.1:5000

## Docker

```bash
docker build -t app .
docker run --rm -p 5000:5000 app
```

Open http://localhost:5000
