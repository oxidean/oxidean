package com.example.app;

import java.util.Map;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class HelloController {
  @GetMapping("/")
  public String home(@RequestParam(defaultValue = "world") String name) {
    return "Hello, " + name + "!";
  }

  @GetMapping("/api/health")
  public Map<String, Object> health() {
    return Map.of("ok", true, "framework", "spring-boot");
  }
}
