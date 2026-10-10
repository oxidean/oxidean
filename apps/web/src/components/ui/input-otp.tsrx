import { OTPField } from "@octanejs/base-ui/otp-field";
import { cn } from "@/lib/utils";

const OTP_LENGTH = 8;
type InputOtpProps = {
  value: string;
  onChange: (value: string) => void;
  onComplete?: (value: string) => void;
  disabled?: boolean;
  id?: string;
  name?: string;
  className?: string;
  "aria-label"?: string;
  "aria-invalid"?: boolean;
  "aria-describedby"?: string;
};

const OTP_SLOTS = Array.from({ length: OTP_LENGTH }, (_, index) => index);

/**
 * Oxidean OTP chrome — Base UI OTPField (8 numeric slots) styled for verify/reset.
 * Prefer this over npm `input-otp` so we stay on the Octane adapter (no ReactCompat).
 */
export function InputOtp({
  value,
  onChange,
  onComplete,
  disabled,
  id,
  name,
  className,
  "aria-label": ariaLabel,
  "aria-invalid": ariaInvalid,
  "aria-describedby": ariaDescribedBy,
}: InputOtpProps) @{
  <OTPField.Root
    id={id}
    name={name}
    length={OTP_LENGTH}
    value={value}
    disabled={disabled}
    invalid={ariaInvalid}
    validationType="numeric"
    autoComplete="one-time-code"
    aria-label={ariaLabel}
    aria-describedby={ariaDescribedBy}
    className={cn(
      "group flex w-full items-center justify-between gap-1 has-[:disabled]:opacity-50",
      className,
    )}
    onValueChange={(next) => {
      onChange(next);
    }}
    onValueComplete={(next) => {
      onComplete?.(next);
    }}
  >
    @for (const index of OTP_SLOTS; key index) {
      <OTPField.Input
        className={cn(
          "relative m-0 box-border flex h-11 w-9 shrink-0 appearance-none items-center justify-center rounded-md border border-input bg-card p-0",
          "text-center text-[14px] font-semibold tabular-nums text-foreground outline-none transition-colors duration-150",
          "focus-visible:z-10 focus-visible:ring-2 focus-visible:ring-ring",
          "disabled:cursor-not-allowed",
          "aria-[invalid=true]:border-destructive",
        )}
        aria-label={index === 0 ? undefined : `Digit ${index + 1} of ${OTP_LENGTH}`}
      />;
    }
  </OTPField.Root>;
}
