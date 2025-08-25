#!/bin/bash

# Add command line argument handling
Y_FLAG=false
if [[ "$1" == "-y" ]]; then
    Y_FLAG=true
fi

echo "🔧 Installing UDIP systemd service..."

# Declare variables
BASE_PROJECT_DIR="$(pwd)/"
TEMPLATE_DIR="${BASE_PROJECT_DIR}templates/"
LOCAL_DIR="${BASE_PROJECT_DIR}services/"

SERVICE_NAME="udip.service"
SERVICE_EXEC="rxt.sh"
TEMP_SERVICE="${LOCAL_DIR}${SERVICE_NAME}"
DEFAULT_USER="$USER"

SYSTEMD_DIR="/etc/systemd/system/"

# verify resources
echo "Verifying resources..."
for file in "$TEMPLATE_DIR"/*; do
    filename=$(basename "$file")

    if [ ! -e "$BASE_PROJECT_DIR/$filename" ]; then
        echo "Resource '$filename' is missing"
        cp "$file" "$BASE_PROJECT_DIR"
        echo "Copied '$filename' to $BASE_PROJECT_DIR"
    fi
done

# Check if template service file exists
if [[ -f "$TEMP_SERVICE" ]]; then
    echo "⚠️  Template service file already exists at: $TEMP_SERVICE"
    if ! $Y_FLAG; then
        read -p "Override it? (y/n): " OVERWRITE
        case "$OVERWRITE" in
            [Yy])
                ;;
            [Nn])
                echo "ℹ️ Keeping existing service template."
                ;;
            *)
                echo "❌ Invalid input. Please enter 'y' or 'n'."
                exit 1
                ;;
        esac
    else
        OVERWRITE="y"
    fi
else
    OVERWRITE="y"
fi

# Create or overwrite the service file
if [[ "$OVERWRITE" =~ ^[Yy]$ ]]; then
    cat <<EOF > "$TEMP_SERVICE"
[Unit]
Description=Universal Dynamic Internet Protocol Resolver Service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=${BASE_PROJECT_DIR}
ExecStart=${LOCAL_DIR}${SERVICE_EXEC}
Restart=on-failure
User=${DEFAULT_USER}

[Install]
WantedBy=multi-user.target
EOF
    echo "✅ Service template created at: $TEMP_SERVICE"
fi

echo "----- start of service file -----"
cat "$TEMP_SERVICE"
echo "-----  end of service file  -----"

if ! $Y_FLAG; then
    read -p "➡️ Press Enter to continue installing..."
fi

# Install and activate the service
for SERVICE_FILE in "$LOCAL_DIR"*.service; do
    BASENAME=$(basename "$SERVICE_FILE")
    TARGET_PATH="${SYSTEMD_DIR}${BASENAME}"

    if [[ -f "$TARGET_PATH" ]]; then
        echo "🟡 $BASENAME already exists in systemd."

        # Reinstall confirmation
        read -p "Uninstall or reinstall or nothing? (u/r/n): " ACTION

        case "$ACTION" in
            [Rr])
                sudo cp "$SERVICE_FILE" "$TARGET_PATH"
                sudo systemctl daemon-reload
                sudo systemctl enable "$BASENAME"
                sudo systemctl restart "$BASENAME"

                echo "✅ Reinstalled and restarted: $BASENAME"
                ;;
            [Uu])
                sudo systemctl stop "$BASENAME"
                sudo systemctl disable "$BASENAME"
                sudo rm "$TARGET_PATH"
                sudo systemctl daemon-reload

                echo "✅ Uninstalled: $BASENAME"
                ;;
            [Nn])
                ;;
            *)
                echo "❌ Invalid input. Please enter 'u' or 'r' or 'n'."
                exit 1
                ;;
        esac
    else
        sudo cp "$SERVICE_FILE" "$TARGET_PATH"
        sudo systemctl daemon-reload
        sudo systemctl enable "$BASENAME"
        sudo systemctl start "$BASENAME"
        echo "✅ Installed and started: $BASENAME"
    fi
done