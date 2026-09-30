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
          glass: 'rgba(30, 41, 59, 0.7)',
        },
        border: '#334155',
        content: '#F1F5F9',
        muted: '#94A3B8',
        faint: '#64748B',
        primary: {
          DEFAULT: '#6366F1',
          hover: '#4F46E5',
          light: '#A5B4FC',
          dark: '#4338CA',
          glow: 'rgba(99, 102, 241, 0.35)',
        },
        success: '#10B981',
        warning: '#F59E0B',
        error: '#EF4444',
        info: '#3B82F6',
        // 服装主题辅助色
        fabric: {
          purple: '#A855F7',
          rose: '#F43F5E',
          amber: '#F59E0B',
          emerald: '#10B981',
          sky: '#0EA5E9',
        },
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
        'card-lg': '16px',
      },
      boxShadow: {
        card: '0 4px 6px rgba(0,0,0,0.3)',
        glow: '0 0 16px rgba(99, 102, 241, 0.3)',
        'glow-lg': '0 0 24px rgba(99, 102, 241, 0.4)',
        'inner-glow': 'inset 0 0 12px rgba(99, 102, 241, 0.08)',
      },
      animation: {
        'pulse-slow': 'pulse 3s cubic-bezier(0.4, 0, 0.6, 1) infinite',
        'float': 'float 3s ease-in-out infinite',
        'glow': 'glow 2s ease-in-out infinite',
      },
      keyframes: {
        float: {
          '0%, 100%': { transform: 'translateY(0px)' },
          '50%': { transform: 'translateY(-4px)' },
        },
        glow: {
          '0%, 100%': { opacity: '0.6' },
          '50%': { opacity: '1' },
        },
      },
      backgroundImage: {
        'gradient-radial': 'radial-gradient(var(--tw-gradient-stops))',
        'gradient-primary': 'linear-gradient(135deg, #6366F1 0%, #8B5CF6 100%)',
        'gradient-brand': 'linear-gradient(135deg, #6366F1 0%, #A855F7 50%, #6366F1 100%)',
      },
    },
  },
  plugins: [],
}
