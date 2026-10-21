# Terraform

Local provider demo (random pet + local file). No cloud credentials required.

## Getting started

```bash
terraform init
terraform plan
terraform apply -auto-approve
cat generated/hello.txt
terraform destroy -auto-approve
```

Copy `terraform.tfvars.example` to `terraform.tfvars` to override variables.
