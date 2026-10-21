variable "greeting" {
  description = "Greeting prefix written to the local file"
  type        = string
  default     = "Hello"
}

variable "pet_length" {
  description = "Number of words in the random pet name"
  type        = number
  default     = 2
}
