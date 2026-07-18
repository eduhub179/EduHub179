-- 0004: create plusnik (records, sheets, tasks)
CREATE TABLE IF NOT EXISTS plusnik_records (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  student_id UUID REFERENCES users(id),
  sheet_id UUID,
  points INTEGER DEFAULT 0,
  created_at TIMESTAMP WITH TIME ZONE DEFAULT now()
);
