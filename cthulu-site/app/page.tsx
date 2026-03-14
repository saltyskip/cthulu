import Nav from "@/components/Nav";
import Hero from "@/components/Hero";
import ValueProps from "@/components/ValueProps";
import MultiAgent from "@/components/MultiAgent";
import UseCases from "@/components/UseCases";
import Skills from "@/components/Skills";
import Plugins from "@/components/Plugins";
import HowItWorks from "@/components/HowItWorks";
import StudioShowcase from "@/components/StudioShowcase";
import ConfigExample from "@/components/ConfigExample";
import GetStarted from "@/components/GetStarted";
import Footer from "@/components/Footer";

export default function Home() {
  return (
    <>
      <Nav />
      <main id="main-content">
        <Hero />
        <ValueProps />
        <MultiAgent />
        <UseCases />
        <Skills />
        <Plugins />
        <HowItWorks />
        <StudioShowcase />
        <ConfigExample />
        <GetStarted />
      </main>
      <Footer />
    </>
  );
}
