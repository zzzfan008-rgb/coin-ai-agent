import { Check, Palette, Shirt, Lightbulb, X, Wrench, Wand2, type LucideIcon } from 'lucide-react'
import type { SkillInfo } from '../api/client'

interface SkillPanelProps {
  skills: SkillInfo[]
  selectedIds: string[]
  onToggle: (id: string) => void
  onClose?: () => void
}

const ICONS: Record<string, LucideIcon> = {
  'fabric-query': Shirt,
  'color-matching': Palette,
  'style-inspiration': Lightbulb,
  'dreamina-cli': Wand2,
}

export function SkillPanel({
  skills,
  selectedIds,
  onToggle,
  onClose,
}: SkillPanelProps) {
  return (
    <div className="flex h-full w-64 flex-col border-l border-border bg-surface">
      <div className="flex items-center justify-between px-4 py-3.5">
        <div className="flex items-center gap-2">
          <Wrench size={16} className="text-primary-light" />
          <h2 className="text-[15px] font-semibold text-content">Skills</h2>
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-faint hover:text-content xl:hidden"
            title="收起 Skill 面板"
          >
            <X size={18} />
          </button>
        )}
      </div>

      <p className="px-4 pb-2 text-xs leading-5 text-faint">
        选择后，Skill 将通过请求的
        <code className="mx-1 rounded bg-surface-elevated px-1 py-0.5 font-mono text-[11px] text-primary-light">
          extra_body.skill_ids
        </code>
        发送给 AI。
      </p>

      <div className="flex-1 space-y-2 overflow-y-auto px-3 py-1">
        {skills.map((skill) => {
          const selected = selectedIds.includes(skill.id)
          const Icon = ICONS[skill.id] ?? Wrench
          return (
            <button
              key={skill.id}
              type="button"
              onClick={() => onToggle(skill.id)}
              className={`block w-full rounded-card border p-3 text-left transition-colors ${
                selected
                  ? 'border-primary bg-primary/10'
                  : 'border-border bg-bg/40 hover:border-faint'
              }`}
            >
              <div className="flex items-center gap-2">
                <span
                  className={`flex h-7 w-7 items-center justify-center rounded-md ${
                    selected
                      ? 'bg-primary text-white'
                      : 'bg-surface-elevated text-primary-light'
                  }`}
                >
                  <Icon size={15} />
                </span>
                <span className="flex-1 text-sm font-medium text-content">
                  {skill.name}
                </span>
                <span
                  className={`flex h-[18px] w-[18px] items-center justify-center rounded-[5px] border transition-colors ${
                    selected
                      ? 'border-primary bg-primary text-white'
                      : 'border-border text-transparent'
                  }`}
                >
                  <Check size={12} />
                </span>
              </div>
              <p className="mt-2 text-xs leading-5 text-muted">
                {skill.description}
              </p>
              <div className="mt-2 flex flex-wrap gap-1">
                {skill.tags.map((tag) => (
                  <span
                    key={tag}
                    className="rounded bg-surface-elevated px-1.5 py-0.5 text-[10px] text-faint"
                  >
                    {tag}
                  </span>
                ))}
              </div>
            </button>
          )
        })}
      </div>
    </div>
  )
}
