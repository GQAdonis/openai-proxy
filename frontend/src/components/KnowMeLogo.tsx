// KnowMe's desktop monogram from desktop/src/shared/components/KnowMeLogo.tsx.
export function KnowMeLogo() {
  return (
    <svg className="brand-logo" viewBox="0 0 200 200" aria-hidden="true">
      <g transform="translate(50, 38)">
        <rect width="20" height="124" rx="10" fill="currentColor" />
        <path
          d="M 16,62 C 26,52 44,32 62,18 C 72,10 80,8 84,14 C 86,18 82,24 76,28 C 60,40 40,56 28,66 C 22,70 18,68 16,64 Z"
          fill="currentColor"
        />
        <path
          d="M 16,62 C 26,72 44,92 62,106 C 72,114 80,116 84,110 C 86,106 82,100 76,96 C 60,84 40,68 28,58 C 22,54 18,56 16,60 Z"
          fill="currentColor"
        />
        <circle cx="18" cy="62" r="10" fill="var(--brand-ember)" />
      </g>
    </svg>
  )
}
