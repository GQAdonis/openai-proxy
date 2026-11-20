import { Slot } from '@radix-ui/react-slot'
import { cva, type VariantProps } from 'class-variance-authority'
import type { ComponentProps } from 'react'

const variants = cva('button', {
  variants: {
    tone: { primary: 'primary', quiet: 'quiet', outline: 'outline' },
  },
  defaultVariants: { tone: 'outline' },
})
export function Button({
  asChild,
  tone,
  className,
  ...props
}: ComponentProps<'button'> &
  VariantProps<typeof variants> & { asChild?: boolean }) {
  const Element = asChild ? Slot : 'button'
  return <Element className={variants({ tone, className })} {...props} />
}
