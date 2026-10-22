defmodule App.Application do
  use Application

  @impl true
  def start(_type, _args) do
    children = [
      {App.Server, port: String.to_integer(System.get_env("PORT") || "4000")}
    ]

    opts = [strategy: :one_for_one, name: App.Supervisor]
    Supervisor.start_link(children, opts)
  end
end
