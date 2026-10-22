from app.main import greet


def test_greet():
    assert greet("Oxidean") == "Hello, Oxidean!"
