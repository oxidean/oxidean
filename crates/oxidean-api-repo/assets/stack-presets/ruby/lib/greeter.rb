# frozen_string_literal: true

module Greeter
  module_function

  def greet(name = "world")
    "Hello, #{name}!"
  end
end
