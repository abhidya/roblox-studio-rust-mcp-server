--!strict
-- Centralised RemoteEvent/RemoteFunction access. Created on the server,
-- waited-for on the client.

local ReplicatedStorage = game:GetService("ReplicatedStorage")

local Remotes = {}

local FOLDER_NAME = "PropHuntRemotes"

local DEFINITIONS = {
	-- server -> client: phase/state changes for HUD
	StateChanged = "RemoteEvent",
	-- client -> server: hider requests to disguise as a prop instance
	RequestDisguise = "RemoteEvent",
	-- server -> client: result/notification toasts
	Notify = "RemoteEvent",
}

function Remotes.setup(): Folder
	local folder = ReplicatedStorage:FindFirstChild(FOLDER_NAME)
	if not folder then
		folder = Instance.new("Folder")
		folder.Name = FOLDER_NAME
		folder.Parent = ReplicatedStorage
	end
	for name, className in DEFINITIONS do
		if not folder:FindFirstChild(name) then
			local remote = Instance.new(className)
			remote.Name = name
			remote.Parent = folder
		end
	end
	return folder
end

function Remotes.get(name: string): Instance
	local folder = ReplicatedStorage:WaitForChild(FOLDER_NAME, 30) :: Folder
	assert(folder, "PropHuntRemotes folder missing")
	return folder:WaitForChild(name, 30)
end

return Remotes
