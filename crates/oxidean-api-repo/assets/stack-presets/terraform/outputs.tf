output "pet_name" {
  description = "Generated pet name"
  value       = random_pet.name.id
}

output "greeting_file" {
  description = "Path to the generated greeting file"
  value       = local_file.greeting.filename
}
