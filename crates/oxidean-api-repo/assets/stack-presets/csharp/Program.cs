using App;

var name = args.Length > 0 ? args[0] : "world";
Console.WriteLine(Greeter.Greet(name));
