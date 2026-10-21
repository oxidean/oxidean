# Django

Manage.py + config package + `core` app (SQLite by default).

## Getting started

```bash
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
python manage.py migrate
python manage.py runserver
```

Open http://127.0.0.1:8000

> `SECRET_KEY` in settings is for local development only — replace before deploy.
