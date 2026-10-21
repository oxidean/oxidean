# frozen_string_literal: true

require "minitest/autorun"
require_relative "../lib/greeter"

class GreeterTest < Minitest::Test
  def test_greet
    assert_equal "Hello, Oxidean!", Greeter.greet("Oxidean")
  end
end
