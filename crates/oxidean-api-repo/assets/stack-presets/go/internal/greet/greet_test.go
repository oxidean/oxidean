package greet

import "testing"

func TestHello(t *testing.T) {
	got := Hello("Go")
	want := "Hello, Go!"
	if got != want {
		t.Fatalf("got %q want %q", got, want)
	}
}
