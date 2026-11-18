CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- =========================================================
-- USERS / ADMINISTRATION
-- =========================================================

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    full_name VARCHAR(150) NOT NULL,
    username VARCHAR(100) NOT NULL UNIQUE,
    email VARCHAR(255) UNIQUE,
    password_hash TEXT NOT NULL,

    role VARCHAR(50) NOT NULL DEFAULT 'admin'
        CHECK (role IN ('admin', 'editor')),

    is_active BOOLEAN NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- SITE SETTINGS
-- =========================================================

CREATE TABLE site_settings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    site_name VARCHAR(255),
    site_description TEXT,

    primary_color VARCHAR(50),
    secondary_color VARCHAR(50),

    contact_phone VARCHAR(50),
    contact_email VARCHAR(255),
    office_address TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- CANDIDATE PROFILE
-- =========================================================

CREATE TABLE candidate_profile (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    full_name VARCHAR(150) NOT NULL,
    position VARCHAR(150),
    constituency VARCHAR(150),

    slogan VARCHAR(255),
    biography TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- CENTRAL MEDIA LIBRARY
-- Images are added as new records and never overwritten.
-- =========================================================

CREATE TABLE media (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    original_filename VARCHAR(500) NOT NULL,
    stored_filename VARCHAR(500) NOT NULL,

    file_path TEXT NOT NULL,
    mime_type VARCHAR(100) NOT NULL,

    file_size BIGINT,

    media_type VARCHAR(50) NOT NULL
        CHECK (media_type IN ('image', 'video', 'document')),

    alt_text VARCHAR(255),
    caption TEXT,

    uploaded_by UUID REFERENCES users(id)
        ON DELETE SET NULL,

    deleted_at TIMESTAMPTZ NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- MEDIA ALBUMS / GALLERY
-- =========================================================

CREATE TABLE media_albums (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    title VARCHAR(255) NOT NULL,
    description TEXT,

    is_published BOOLEAN NOT NULL DEFAULT TRUE,
    display_order INTEGER NOT NULL DEFAULT 0,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE album_media (
    album_id UUID NOT NULL REFERENCES media_albums(id)
        ON DELETE CASCADE,

    media_id UUID NOT NULL REFERENCES media(id)
        ON DELETE CASCADE,

    display_order INTEGER NOT NULL DEFAULT 0,

    PRIMARY KEY (album_id, media_id)
);

-- =========================================================
-- HOMEPAGE HEADLINES / FEATURED CONTENT
-- =========================================================

CREATE TABLE headlines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    title VARCHAR(255) NOT NULL,
    description TEXT,

    media_id UUID REFERENCES media(id)
        ON DELETE SET NULL,

    link_url VARCHAR(500),

    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    display_order INTEGER NOT NULL DEFAULT 0,

    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- NAVIGATION
-- =========================================================

CREATE TABLE navigation_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    title VARCHAR(150) NOT NULL,
    url VARCHAR(500) NOT NULL,

    display_order INTEGER NOT NULL DEFAULT 0,
    is_visible BOOLEAN NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- MANIFESTO
-- =========================================================

CREATE TABLE manifesto_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    title VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL UNIQUE,

    description TEXT NOT NULL,

    media_id UUID REFERENCES media(id)
        ON DELETE SET NULL,

    display_order INTEGER NOT NULL DEFAULT 0,

    status VARCHAR(50) NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'published')),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- NEWS / UPDATES
-- =========================================================

CREATE TABLE news (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    title VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL UNIQUE,

    content TEXT NOT NULL,

    featured_media_id UUID REFERENCES media(id)
        ON DELETE SET NULL,

    status VARCHAR(50) NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'published')),

    published_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- EVENTS
-- =========================================================

CREATE TABLE events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    title VARCHAR(255) NOT NULL,
    description TEXT,

    location VARCHAR(255),

    event_date DATE NOT NULL,

    start_time TIME,
    end_time TIME,

    media_id UUID REFERENCES media(id)
        ON DELETE SET NULL,

    status VARCHAR(50) NOT NULL DEFAULT 'upcoming'
        CHECK (status IN ('upcoming', 'completed', 'cancelled')),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- CAMPAIGN PROGRAMME / SCHEDULE
-- =========================================================

CREATE TABLE campaign_schedule (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    title VARCHAR(255) NOT NULL,
    description TEXT,

    location VARCHAR(255),

    scheduled_date DATE NOT NULL,
    scheduled_time TIME,

    status VARCHAR(50) NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'completed', 'cancelled')),

    display_order INTEGER NOT NULL DEFAULT 0,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- POSTER CATEGORIES
-- =========================================================

CREATE TABLE poster_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    name VARCHAR(150) NOT NULL UNIQUE,
    description TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- CAMPAIGN POSTERS
-- =========================================================

CREATE TABLE campaign_posters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    category_id UUID REFERENCES poster_categories(id)
        ON DELETE SET NULL,

    title VARCHAR(255) NOT NULL,
    description TEXT,

    preview_media_id UUID REFERENCES media(id)
        ON DELETE SET NULL,

    download_media_id UUID REFERENCES media(id)
        ON DELETE SET NULL,

    is_published BOOLEAN NOT NULL DEFAULT FALSE,

    download_count BIGINT NOT NULL DEFAULT 0,

    published_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- TALENT CATEGORIES
-- =========================================================

CREATE TABLE talent_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    name VARCHAR(150) NOT NULL UNIQUE,
    description TEXT,

    is_active BOOLEAN NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- YOUTH TALENT APPLICATIONS
-- =========================================================

CREATE TABLE talent_applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    full_name VARCHAR(150) NOT NULL,

    age INTEGER,

    phone VARCHAR(50) NOT NULL,
    email VARCHAR(255),

    location VARCHAR(255),

    category_id UUID REFERENCES talent_categories(id)
        ON DELETE SET NULL,

    talent_description TEXT NOT NULL,

    achievements TEXT,

    support_needed TEXT,

    status VARCHAR(50) NOT NULL DEFAULT 'new'
        CHECK (
            status IN (
                'new',
                'under_review',
                'contacted',
                'supported',
                'rejected',
                'archived'
            )
        ),

    admin_notes TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE talent_application_media (
    application_id UUID NOT NULL
        REFERENCES talent_applications(id)
        ON DELETE CASCADE,

    media_id UUID NOT NULL
        REFERENCES media(id)
        ON DELETE CASCADE,

    PRIMARY KEY (application_id, media_id)
);

-- =========================================================
-- DOCUMENTS
-- =========================================================

CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    title VARCHAR(255) NOT NULL,
    description TEXT,

    media_id UUID REFERENCES media(id)
        ON DELETE SET NULL,

    is_published BOOLEAN NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- SOCIAL MEDIA LINKS
-- =========================================================

CREATE TABLE social_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    platform VARCHAR(100) NOT NULL,
    url VARCHAR(500) NOT NULL,

    display_order INTEGER NOT NULL DEFAULT 0,
    is_visible BOOLEAN NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- CONTACT MESSAGES
-- =========================================================

CREATE TABLE contact_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    full_name VARCHAR(150) NOT NULL,
    email VARCHAR(255),
    phone VARCHAR(50),

    subject VARCHAR(255),
    message TEXT NOT NULL,

    status VARCHAR(50) NOT NULL DEFAULT 'unread'
        CHECK (status IN ('unread', 'read', 'archived')),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =========================================================
-- USEFUL INDEXES
-- =========================================================

CREATE INDEX idx_news_status
ON news(status);

CREATE INDEX idx_events_date
ON events(event_date);

CREATE INDEX idx_campaign_schedule_date
ON campaign_schedule(scheduled_date);

CREATE INDEX idx_media_type
ON media(media_type);

CREATE INDEX idx_talent_applications_status
ON talent_applications(status);
