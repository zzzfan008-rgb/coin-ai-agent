/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        bg: '#0F172A',
        surface: {
          DEFAULT: '#1E293B',
          elevated: '#273449',
        },
        border: '#334155',
        content: '#F1F5F9',
        muted: '#94A3B8',
        faint: '#64748B',
        primary: {
          DEFAULT: '#6366F1',
          hover: '#4F46E5',
          light: '#A5B4FC',
        },
        success: '#10B981',
        warning: '#F59E0B',
        error: '#EF4444',
        info: '#3B82F6',
      },
      fontFamily: {
        sans: [
          'Inter',
          '"Noto Sans SC"',
          '"PingFang SC"',
          '"Microsoft YaHei"',
          'system-ui',
          'sans-serif',
        ],
        mono: ['"JetBrains Mono"', '"Fira Code"', 'monospace'],
      },
      borderRadius: {
        card: '12px',
      },
      boxShadow: {
        card: '0 4px 6px rgba(0,0,0,0.3)',
      },
    },
  },
  plugins: [],
}
