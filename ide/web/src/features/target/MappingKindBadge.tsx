import { MAPPING_KIND_META } from "@/features/target/mappingKindMeta";
import type { TargetMappingKind } from "@/features/target/targetMapping";

type MappingKindBadgeProps = {
  kind: TargetMappingKind;
  compact?: boolean;
};

export function MappingKindBadge({ kind, compact }: MappingKindBadgeProps) {
  const meta = MAPPING_KIND_META[kind];

  return (
    <span className={`mapping-kind-badge symbol-kind ${meta.tone}`} title={meta.label}>
      {compact ? meta.short : meta.label}
    </span>
  );
}
