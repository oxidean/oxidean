package com.example.app;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.content;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.WebMvcTest;
import org.springframework.test.web.servlet.MockMvc;

@WebMvcTest(HelloController.class)
class HelloControllerTest {
  @Autowired
  private MockMvc mockMvc;

  @Test
  void home() throws Exception {
    mockMvc.perform(get("/").param("name", "Oxidean"))
        .andExpect(status().isOk())
        .andExpect(content().string("Hello, Oxidean!"));
  }
}
