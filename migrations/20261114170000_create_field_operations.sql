BEGIN;

CREATE TABLE campaign_areas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(150) NOT NULL,
    ward VARCHAR(150) NOT NULL,
    village VARCHAR(150),
    description TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE editor_area_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    editor_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    area_id UUID NOT NULL REFERENCES campaign_areas(id) ON DELETE RESTRICT,
    assigned_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(editor_id, area_id)
);

CREATE TABLE field_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title VARCHAR(200) NOT NULL,
    description TEXT,
    area_id UUID REFERENCES campaign_areas(id) ON DELETE RESTRICT,
    assigned_to UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    assigned_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    due_at TIMESTAMPTZ,
    status VARCHAR(30) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'in_progress', 'completed', 'cancelled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE field_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id UUID REFERENCES field_tasks(id) ON DELETE SET NULL,
    editor_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    area_id UUID NOT NULL REFERENCES campaign_areas(id) ON DELETE RESTRICT,
    report_text TEXT NOT NULL,
    status VARCHAR(30) NOT NULL DEFAULT 'submitted'
        CHECK (status IN ('submitted', 'reviewed')),
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    reviewed_at TIMESTAMPTZ
);

CREATE TABLE announcements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title VARCHAR(200) NOT NULL,
    body TEXT NOT NULL,
    published_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_area_assignments_editor
    ON editor_area_assignments(editor_id);

CREATE INDEX idx_area_assignments_area
    ON editor_area_assignments(area_id);

CREATE INDEX idx_field_tasks_assigned_to
    ON field_tasks(assigned_to);

CREATE INDEX idx_field_tasks_status
    ON field_tasks(status);

CREATE INDEX idx_field_reports_editor
    ON field_reports(editor_id);

CREATE INDEX idx_field_reports_area
    ON field_reports(area_id);

CREATE INDEX idx_announcements_active
    ON announcements(is_active, published_at);

COMMIT;