from flask import Blueprint, jsonify, render_template

bp = Blueprint("main", __name__)


@bp.get("/")
def index():
    return render_template("index.html", message="Hello from Flask!")


@bp.get("/up")
def health():
    return jsonify(status="ok")
