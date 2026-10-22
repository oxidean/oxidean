terraform {
  required_version = ">= 1.5.0"

  required_providers {
    local = {
      source  = "hashicorp/local"
      version = "~> 2.5"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
  }
}

resource "random_pet" "name" {
  length = var.pet_length
}

resource "local_file" "greeting" {
  filename = "${path.module}/generated/hello.txt"
  content  = "${var.greeting}, ${random_pet.name.id}!\n"
}
