defmodule App.Server do
  use GenServer

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @impl true
  def init(opts) do
    port = Keyword.fetch!(opts, :port)
    {:ok, listen} = :gen_tcp.listen(port, [:binary, packet: :raw, active: false, reuseaddr: true])
    IO.puts("Listening on http://localhost:#{port}")
    send(self(), :accept)
    {:ok, %{listen: listen}}
  end

  @impl true
  def handle_info(:accept, %{listen: listen} = state) do
    {:ok, socket} = :gen_tcp.accept(listen)
    spawn(fn -> serve(socket) end)
    send(self(), :accept)
    {:noreply, state}
  end

  defp serve(socket) do
    case :gen_tcp.recv(socket, 0) do
      {:ok, _request} ->
        body = App.greet("Elixir")
        response = """
        HTTP/1.1 200 OK\r
        Content-Type: text/plain; charset=utf-8\r
        Content-Length: #{byte_size(body)}\r
        Connection: close\r
        \r
        #{body}
        """
        :gen_tcp.send(socket, response)
        :gen_tcp.close(socket)

      {:error, _} ->
        :gen_tcp.close(socket)
    end
  end
end
