import { Component, type ReactNode } from "react";
import { log } from "../api/logger";
import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  error: Error | null;
}

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    log("error", "[ErrorBoundary] Caught error", `${error.message}\n${info.componentStack}`);
  }

  render() {
    if (this.state.error) {
      return (
        this.props.fallback || (
          <div className="canvas-container">
            <div className="empty-state">
              <p>Something went wrong</p>
              <p style={{ fontSize: 12, color: "var(--text-secondary)", maxWidth: 400, textAlign: "center" }}>
                {this.state.error.message}
              </p>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => this.setState({ error: null })}
              >
                Try Again
              </Button>
            </div>
          </div>
        )
      );
    }
    return this.props.children;
  }
}
