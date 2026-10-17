-- ============================================================================
-- FILE: 0008_create_agent_user.sql
-- PURPOSE: Service user of the chat agent. Homework the agent finds in class chats
--          without a known author is written on its behalf (AGENT_USER_ID in agent/.env).
-- DEPENDENCIES: [0001_create_users.sql]
-- ============================================================================

-- Fixed id, so every database (local, test, server) has the same AGENT_USER_ID.
-- No password: nobody logs in as the agent.
INSERT INTO users (user_id, login, role, last_name, first_name)
VALUES ('00000000-0000-0000-0000-0000000000c1', 'agent', 'teacher', 'EduHub179', 'Агент')
ON CONFLICT DO NOTHING;
